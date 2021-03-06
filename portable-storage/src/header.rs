use bytes::{Buf, BufMut, ByteOrder, BytesMut};
use errors::Result;

pub const PORTABLE_STORAGE_SIGNATUREA: u32 = 0x01011101;
pub const PORTABLE_STORAGE_SIGNATUREB: u32 = 0x01020101;
pub const PORTABLE_STORAGE_FORMAT_VER: u8 = 1;
pub const PORTABLE_STORAGE_BLOCK_HEADER_LENGTH: usize = 4 + 4 + 1;

#[derive(Debug)]
pub struct StorageBlockHeader {
    pub signature_a: u32,
    pub signature_b: u32,
    pub version: u8,
}

impl StorageBlockHeader {
    pub fn is_valid_signature_a(&self) -> bool {
        self.signature_a == PORTABLE_STORAGE_SIGNATUREA
    }

    pub fn is_valid_signature_b(&self) -> bool {
        self.signature_a == PORTABLE_STORAGE_SIGNATUREB
    }

    pub fn is_valid_version(&self) -> bool {
        self.version == PORTABLE_STORAGE_FORMAT_VER
    }

    pub fn read<T: ByteOrder, B: Buf>(buf: &mut B) -> Result<Self> {
        ensure_eob!(buf, PORTABLE_STORAGE_BLOCK_HEADER_LENGTH);

        Ok(StorageBlockHeader {
            signature_a: buf.get_u32::<T>(),
            signature_b: buf.get_u32::<T>(),
            version: buf.get_u8(),
        })
    }

    pub fn write<T: ByteOrder>(buf: &mut BytesMut) {
        buf.reserve(PORTABLE_STORAGE_BLOCK_HEADER_LENGTH);
        buf.put_u32::<T>(PORTABLE_STORAGE_SIGNATUREA);
        buf.put_u32::<T>(PORTABLE_STORAGE_SIGNATUREB);
        buf.put_u8(PORTABLE_STORAGE_FORMAT_VER);
    }
}
