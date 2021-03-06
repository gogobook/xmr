use std::mem::size_of;
use std::collections::LinkedList;
use std::marker::PhantomData;

use failure::Error;
use bytes::{ByteOrder, BytesMut, IntoBuf, Buf};

use portable_storage::{Result, StorageEntry};
use portable_storage::ser::ToUnderlying;
use portable_storage::ser::bytes::SerializeBytes;
use portable_storage::errors::InvalidStorageEntry;

use levin::DefaultEndian;

pub type DefaultSerializableLinkedList<T> = SerializableLinkedList<DefaultEndian, T>;

#[derive(Debug, Default, Clone)]
pub struct SerializableLinkedList<E: ByteOrder, T: Sized + SerializeBytes + Default>(pub LinkedList<T>, PhantomData<E>);

impl<E: ByteOrder, T: Sized + SerializeBytes + Default> ToUnderlying for SerializableLinkedList<E, T> {
    fn to_underlying(entry: &StorageEntry) -> Result<SerializableLinkedList<E, T>> {
        match entry {
            &StorageEntry::Buf(ref v) => {
                let mut remaining = v.len();
                let mut buf = v.into_buf();
                let mut list = SerializableLinkedList::default();
                while remaining > 0 {
                    let r = buf.remaining();
                    let element = T::from_bytes::<E, _>(&mut buf)?;
                    list.0.push_front(element);
                    remaining -= r - buf.remaining();
                }
                Ok(list)
            }
            _ => Err(Error::from(InvalidStorageEntry::new("StorageEntry::Buf")))
        }
    }
}

impl<E: ByteOrder, T: Sized + SerializeBytes + Default> From<SerializableLinkedList<E, T>> for StorageEntry {
    fn from(v: SerializableLinkedList<E, T>) -> StorageEntry {
        assert!(v.0.len() > 0);

        let mut buf = Vec::with_capacity(size_of::<T>() * v.0.len());
        let mut write_buf = BytesMut::new();
        for item in v.0.iter() {
            item.to_bytes::<E>(&mut write_buf);
            buf.extend_from_slice(write_buf.as_ref());
            write_buf.clear();
        }

        StorageEntry::Buf(buf)
    }
}
