mod read_channel;
mod write_channel;

use crossbeam::channel::{unbounded, Receiver, Sender, TryRecvError};
use std::collections::HashMap;

pub use read_channel::ReadChannel;
pub use write_channel::WriteChannel;

pub use crate::packet::{
    DataVersion, Packet, PacketError, PacketView, UntypedPacket, UntypedPacketCast,
};
pub use read_channel::PacketSet;

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Clone)]
pub enum ChannelError {
    #[error("Data was received in channel {0:?} with an already existing version.")]
    DuplicateDataVersionError(PacketBufferAddress),
    #[error("Trying to use a channel which does not exist, channel id {0:?}")]
    MissingChannel(ChannelID),
    #[error("Trying to use a channel index which does not exist, channel index {0:?}")]
    MissingChannelIndex(usize),
    #[error("Channel has no data {0:?}")]
    MissingChannelData(usize),
    #[error(transparent)]
    ReceiveError(#[from] TryRecvError),
    #[error("Error while sending data {0}")]
    SendError(String),
    #[error(transparent)]
    PacketError(#[from] PacketError),
    #[error("No more data to send. Closing channel.")]
    EndOfStreamError(ChannelID),
}

#[derive(Eq, Hash, Debug, Clone)]
pub struct ChannelID {
    id: String,
}

impl ChannelID {
    pub fn new(id: String) -> Self {
        ChannelID { id }
    }
    pub fn from(id: &str) -> Self {
        ChannelID { id: id.to_string() }
    }
}

impl PartialEq for ChannelID {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

type PacketBufferAddress = (ChannelID, DataVersion);
struct PacketWithAddress(PacketBufferAddress, UntypedPacket);

#[derive(Default)]
pub struct BufferedReadData {
    data: HashMap<PacketBufferAddress, UntypedPacket>,
}

impl BufferedReadData {
    pub fn new() -> BufferedReadData {
        BufferedReadData {
            data: HashMap::<PacketBufferAddress, UntypedPacket>::default(),
        }
    }

    pub fn insert(&mut self, channel: &ChannelID, packet: UntypedPacket) {
        let data_version = (channel.clone(), packet.version.clone());
        self.data.insert(data_version, packet);
    }

    pub fn has_version(&self, channel: &ChannelID, version: &DataVersion) -> bool {
        let data_version = (channel.clone(), version.clone());
        self.data.contains_key(&data_version)
    }

    pub fn remove_version(
        &mut self,
        channel: &ChannelID,
        version: &DataVersion,
    ) -> Option<UntypedPacket> {
        let data_version = (channel.clone(), version.clone());
        self.data.remove(&data_version)
    }
}

pub fn untyped_channel() -> (UntypedSenderChannel, UntypedReceiverChannel) {
    let (channel_sender, channel_receiver) = unbounded::<UntypedPacket>();
    return (
        UntypedSenderChannel::new(&channel_sender.clone()),
        UntypedReceiverChannel::new(&channel_receiver.clone()),
    );
}

#[derive(Debug)]
pub struct UntypedReceiverChannel {
    receiver: Receiver<UntypedPacket>,
}

impl UntypedReceiverChannel {
    pub fn new(receiver: &Receiver<UntypedPacket>) -> Self {
        UntypedReceiverChannel {
            receiver: receiver.clone() as Receiver<UntypedPacket>,
        }
    }
    pub fn try_receive(&self) -> Result<UntypedPacket, ChannelError> {
        match self.receiver.try_recv() {
            Ok(packet) => Ok(packet),
            Err(error) => Err(ChannelError::ReceiveError(error)),
        }
    }
}

#[derive(Debug)]
pub struct UntypedSenderChannel {
    sender: Sender<UntypedPacket>,
}

impl UntypedSenderChannel {
    pub fn new(sender: &Sender<UntypedPacket>) -> Self {
        UntypedSenderChannel {
            sender: sender.clone() as Sender<UntypedPacket>,
        }
    }
    pub fn send<T: 'static>(&self, data: Packet<T>) -> Result<(), ChannelError> {
        match self.sender.send(data.to_untyped()) {
            Ok(res) => Ok(res),
            Err(_err) => {
                return Err(ChannelError::SendError(
                    "Could not send because the channel is disconnected".to_string(),
                ));
            }
        }
    }
}
