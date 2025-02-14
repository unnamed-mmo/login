use crate::models::header::Header;
use actix::prelude::*;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::io;
use std::io::{Cursor, Read};
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::oneshot::Sender;

use super::client_update_data::ClientUpdateData;
use super::packet_types::PacketType;

#[derive(Debug, Message, Clone)]
#[rtype(result = "()")]
pub struct Packet {
    pub header: Header,
    pub body: Arc<dyn PacketData>,
}

pub enum MessageType {
    Acknowledgment,
    Request,
    Event,
    Command,
    Error,
    Data,
    Outgoing,
}

// this is the trait that allows for serializing and deserializing the packet's data
pub trait PacketData: std::fmt::Debug + Send + Sync + 'static {
    fn from_bytes(bytes: &[u8]) -> io::Result<Self>
    where
        Self: Sized;
    fn to_bytes(&self) -> Vec<u8>;
    fn on_receive(
        &self,
        queue: Arc<Mutex<HashMap<u32, Sender<()>>>>,
        update_stream: Arc<Mutex<Vec<ClientUpdateData>>>,
    ) -> BoxFuture<'static, ()>;
    fn message_type(&self) -> MessageType;
}

impl Packet {
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        let header = Header::try_from_bytes(bytes).unwrap();
        // if the packet has a body, add the body to the packet
        let body = if header.size.unwrap_or(0) < bytes.len() {
            &bytes[header.size.unwrap_or(0)..]
        } else {
            &[]
        };

        let body_bytes = if header.zerocoded {
            zero_decode(body)
        } else {
            body.to_vec() // Convert slice to Vec<u8>
        };

        let body =
            PacketType::from_id(header.id, header.frequency, body_bytes.as_slice())?.into_arc();
        Ok(Self { header, body })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(self.header.to_bytes());
        bytes.extend(self.body.to_bytes());
        bytes
    }
}

fn zero_decode(bytes: &[u8]) -> Vec<u8> {
    let mut cursor = Cursor::new(bytes);
    let mut dest = Vec::new();

    while cursor.position() < bytes.len() as u64 {
        let mut byte = [0u8; 1];
        cursor.read_exact(&mut byte).unwrap();
        let byte = byte[0];

        if byte == 0x00 {
            let mut repeat_count = [0u8; 1];
            cursor.read_exact(&mut repeat_count).unwrap();
            let repeat_count = repeat_count[0] as usize;

            dest.extend(vec![0x00; repeat_count]);
        } else {
            dest.push(byte);
        }
    }
    dest
}

fn zero_encode(src: &[u8]) -> Vec<u8> {
    let mut dest = Vec::new();
    let mut i = 0;

    while i < src.len() {
        if src[i] == 0x00 {
            // Count consecutive zeros
            let mut count = 1;
            while i + count < src.len() && src[i + count] == 0x00 {
                count += 1;
            }

            // Add a zero byte and the repeat count to the destination
            dest.push(0x00);
            dest.push(count as u8);
            i += count;
        } else {
            // Add non-zero byte to the destination
            dest.push(src[i]);
            i += 1;
        }
    }

    dest
}
