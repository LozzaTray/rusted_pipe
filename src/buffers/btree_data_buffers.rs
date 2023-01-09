use super::BufferError;
use super::DataBuffer;
use super::OrderedBuffer;
use crate::buffers::single_buffers::FixedSizeBTree;
use crate::buffers::single_buffers::FixedSizeBuffer;
use crate::packet::ChannelID;
use crate::packet::DataVersion;
use crate::packet::UntypedPacket;

use itertools::Itertools;

use super::PacketBufferAddress;
use std::collections::HashMap;

pub struct BtreeBufferedData {
    data: HashMap<ChannelID, FixedSizeBTree>,
    max_size: usize,
}

impl BtreeBufferedData {
    pub fn new(max_size: usize) -> Self {
        BtreeBufferedData {
            data: Default::default(),
            max_size,
        }
    }

    fn get_channel(&mut self, channel: &ChannelID) -> Result<&mut FixedSizeBTree, BufferError> {
        Ok(self
            .data
            .get_mut(channel)
            .ok_or(BufferError::InternalError(format!(
                "Cannod find channel {}",
                channel.id
            )))?)
    }

    fn get_or_create_channel(&mut self, channel: &ChannelID) -> &mut FixedSizeBTree {
        self.data
            .entry(channel.clone())
            .or_insert(FixedSizeBTree::default())
    }
}

impl DataBuffer for BtreeBufferedData {
    fn insert(
        &mut self,
        channel: &ChannelID,
        packet: UntypedPacket,
    ) -> Result<PacketBufferAddress, BufferError> {
        if self.has_version(&channel, &packet.version) {
            return Err(BufferError::DuplicateDataVersionError((
                channel.clone(),
                packet.version.clone(),
            )));
        }

        let buffer = self.get_channel(channel)?;
        let data_version = (channel.clone(), packet.version.clone());
        buffer.insert(packet.version.clone(), packet);
        Ok(data_version)
    }

    fn consume(
        &mut self,
        version: &PacketBufferAddress,
    ) -> Result<Option<UntypedPacket>, BufferError> {
        let data = self.get_channel(&version.0)?.remove(&version.1);
        self.get_channel(&version.0)?.cleanup_before(&version.1);
        Ok(data)
    }

    fn get(
        &mut self,
        version: &PacketBufferAddress,
    ) -> Result<Option<&UntypedPacket>, BufferError> {
        Ok(self.get_channel(&version.0)?.get(&version.1))
    }

    fn available_channels(&self) -> Vec<ChannelID> {
        self.data
            .keys()
            .into_iter()
            .map(|key| key.clone())
            .collect_vec()
    }

    fn create_channel(&mut self, channel: &ChannelID) -> Result<ChannelID, BufferError> {
        if self.data.contains_key(channel) {
            return Err(BufferError::DuplicateChannelError(channel.clone()));
        }
        self.get_or_create_channel(channel);
        Ok(channel.clone())
    }
}

impl OrderedBuffer for BtreeBufferedData {
    fn has_version(&self, channel: &ChannelID, version: &DataVersion) -> bool {
        self.data.contains_key(channel) && self.data.get(channel).unwrap().contains_key(version)
    }
}

#[cfg(test)]
mod btree_buffer_tests {
    use rand::seq::SliceRandom;

    use super::*;
    use crate::channels::Packet;
    use crate::packet::UntypedPacketCast;

    #[test]
    fn test_buffer_errors_if_inserts_on_missing_channel() {
        let max_size = 20;
        let mut buffer = BtreeBufferedData::new(max_size);

        let channel_0 = ChannelID {
            id: "ch0".to_string(),
        };
        let channel_1 = ChannelID {
            id: "ch1".to_string(),
        };
        buffer.create_channel(&channel_1).unwrap();
        let version = DataVersion { timestamp: 1 };

        let packet = Packet::<String>::new("test".to_string(), version.clone());
        assert!(buffer.insert(&channel_0, packet.to_untyped()).is_err())
    }

    #[test]
    fn test_buffer_throws_if_same_channel_created() {
        let max_size = 20;
        let mut buffer = BtreeBufferedData::new(max_size);

        let channel_0 = ChannelID {
            id: "ch0".to_string(),
        };
        assert!(buffer.create_channel(&channel_0).is_ok());
        assert!(buffer.create_channel(&channel_0).is_err());
    }

    #[test]
    fn test_buffer_inserts_returns_data_and_gets_retained() {
        let max_size = 20;
        let mut buffer = BtreeBufferedData::new(max_size);

        let channel_0 = ChannelID {
            id: "ch0".to_string(),
        };
        let channel_1 = ChannelID {
            id: "ch1".to_string(),
        };
        buffer.create_channel(&channel_0).unwrap();
        buffer.create_channel(&channel_1).unwrap();
        let version = DataVersion { timestamp: 1 };

        let packet = Packet::<String>::new("test".to_string(), version.clone());
        buffer.insert(&channel_0, packet.to_untyped()).unwrap();
        for _i in 0..2 {
            let untyped = buffer
                .get(&(channel_0.clone(), version.clone()))
                .unwrap()
                .unwrap();
            let data = untyped.deref::<String>().unwrap();
            assert_eq!(*data.data, "test");
        }

        assert!(buffer
            .get(&(channel_1.clone(), version.clone()))
            .unwrap()
            .is_none())
    }

    #[test]
    fn test_buffer_insert_random_order_then_removes_old_data_once_consumed() {
        let max_size = 20;
        let mut buffer = BtreeBufferedData::new(max_size);

        let channel_0 = ChannelID {
            id: "ch0".to_string(),
        };
        let channel_1 = ChannelID {
            id: "ch1".to_string(),
        };
        buffer.create_channel(&channel_0).unwrap();
        buffer.create_channel(&channel_1).unwrap();

        let mut vals: Vec<u64> = (0..100).collect();
        vals.shuffle(&mut rand::thread_rng());

        for i in vals {
            let version = DataVersion { timestamp: i };
            let packet = Packet::<String>::new("test_0".to_string(), version.clone());
            buffer.insert(&channel_0, packet.to_untyped()).unwrap();
            let packet = Packet::<String>::new("test_1".to_string(), version.clone());
            buffer.insert(&channel_1, packet.to_untyped()).unwrap();
        }
        let version = DataVersion { timestamp: 10 };
        let address = (channel_0.clone(), version.clone());
        let _ = buffer.consume(&address).unwrap().unwrap();

        for old_version in 0..9 {
            let version = DataVersion {
                timestamp: old_version,
            };
            let address = (channel_0.clone(), version.clone());
            assert!(buffer.get(&address).unwrap().is_none())
        }
    }
}
