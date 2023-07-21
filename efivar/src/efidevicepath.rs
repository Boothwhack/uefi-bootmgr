use std::error::Error;
use std::io;
use std::io::{Read, Write};
use std::iter::Sum;
use bytemuck::cast_slice;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum DevicePathProtocolParseError {
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error("unknown device path type {0:02X}")]
    UnknownType(u8),
    #[error("type {typ} encountered unknown subtype {sub_type:02X}")]
    UnknownSubType {
        typ: &'static str,
        sub_type: u8,
    },
    #[error("error parsing subtype {sub_type}, {message}: {source:?}")]
    ParseSubType { sub_type: String, message: String, source: Option<Box<dyn Error>> },
}

pub type Result<T> = std::result::Result<T, DevicePathProtocolParseError>;

#[derive(Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum EFIDevicePathProtocol {
    MediaDevicePath(MediaDevicePath) = EFIDevicePathProtocol::MEDIA_DEVICE_PATH,
    End(EndSubType) = EFIDevicePathProtocol::END_OF_HARDWARE_DEVICE_PATH,
}

impl<'a> Sum<&'a EFIDevicePathProtocol> for u16 {
    fn sum<I: Iterator<Item=&'a EFIDevicePathProtocol>>(iter: I) -> Self {
        iter.map(|value| value.size()).sum()
    }
}

impl EFIDevicePathProtocol {
    const MEDIA_DEVICE_PATH: u8 = 0x04;
    const END_OF_HARDWARE_DEVICE_PATH: u8 = 0x7F;

    pub fn new_hard_drive_gpt(partition_number: u32, partition_start: u64, partition_size: u64, uuid: Uuid) -> Self {
        EFIDevicePathProtocol::MediaDevicePath(MediaDevicePath::HardDrive(HardDriveDevicePath::new_gpt(partition_number, partition_start, partition_size, uuid)))
    }

    pub fn new_file_path(path: impl Into<String>) -> Self {
        EFIDevicePathProtocol::MediaDevicePath(MediaDevicePath::FilePath(FilePathDevicePath {
            path_name: path.into()
        }))
    }

    pub fn new_end_entire() -> Self {
        EFIDevicePathProtocol::End(EndSubType::EndEntireDevicePath)
    }

    pub fn size(&self) -> u16 {
        4 + match self {
            EFIDevicePathProtocol::MediaDevicePath(value) => value.size(),
            EFIDevicePathProtocol::End(_) => 0,
        }
    }

    pub fn parse(read: &mut impl Read) -> Result<Self> {
        let typ = read.read_u8()?;
        let sub_type = read.read_u8()?;
        let _length = read.read_u16::<LittleEndian>()?;
        match typ {
            Self::MEDIA_DEVICE_PATH => Ok(EFIDevicePathProtocol::MediaDevicePath(MediaDevicePath::parse(sub_type, read)?)),
            Self::END_OF_HARDWARE_DEVICE_PATH => {
                Ok(EFIDevicePathProtocol::End(sub_type.try_into().map_err(|_| DevicePathProtocolParseError::UnknownSubType {
                    typ: "End",
                    sub_type,
                })?))
            }
            _ => Err(DevicePathProtocolParseError::UnknownType(typ)),
        }
    }

    pub fn write(&self, write: &mut impl Write) -> io::Result<()> {
        let (typ, sub_type) = match self {
            EFIDevicePathProtocol::MediaDevicePath(value) => (Self::MEDIA_DEVICE_PATH, value.sub_type()),
            EFIDevicePathProtocol::End(value) => (Self::END_OF_HARDWARE_DEVICE_PATH, value.sub_type()),
        };

        write.write_u8(typ)?;
        write.write_u8(sub_type)?;

        write.write_u16::<LittleEndian>(self.size())?;

        match self {
            EFIDevicePathProtocol::MediaDevicePath(media) => media.write(write)?,
            EFIDevicePathProtocol::End(_) => (),
        };

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum MediaDevicePath {
    HardDrive(HardDriveDevicePath) = MediaDevicePath::HARD_DRIVE_SUBTYPE,
    FilePath(FilePathDevicePath) = MediaDevicePath::FILEPATH_SUBTYPE,
}

impl MediaDevicePath {
    const HARD_DRIVE_SUBTYPE: u8 = 0x01;
    const CDROM_SUBTYPE: u8 = 0x02;
    const VENDOR_SUBTYPE: u8 = 0x03;
    const FILEPATH_SUBTYPE: u8 = 0x04;
    const PROTOCOL_SUBTYPE: u8 = 0x05;
    const PIWG_FIRMWARE_FILE_SUBTYPE: u8 = 0x06;
    const PIWG_FIRMWARE_VOL_SUBTYPE: u8 = 0x07;
    const RELATIVE_OFFSET_RANGE_SUBTYPE: u8 = 0x08;
    const RAM_DISK_SUBTYPE: u8 = 0x09;

    pub fn parse(sub_type: u8, read: &mut impl Read) -> Result<Self> {
        match sub_type {
            Self::HARD_DRIVE_SUBTYPE => Ok(MediaDevicePath::HardDrive(HardDriveDevicePath::parse(read)?)),
            Self::FILEPATH_SUBTYPE => Ok(MediaDevicePath::FilePath(FilePathDevicePath::parse(read)?)),
            _ => Err(DevicePathProtocolParseError::UnknownSubType { typ: "MediaDevicePath", sub_type }),
        }
    }

    pub fn write(&self, write: &mut impl Write) -> io::Result<()> {
        match self {
            MediaDevicePath::HardDrive(value) => value.write(write)?,
            MediaDevicePath::FilePath(FilePathDevicePath { path_name }) => {
                let path_name = path_name.encode_utf16().chain([0x0000]).collect::<Vec<_>>();
                write.write_all(cast_slice(path_name.as_slice()))?;
            }
        }

        Ok(())
    }

    pub fn size(&self) -> u16 {
        match self {
            MediaDevicePath::HardDrive(HardDriveDevicePath { .. }) => 4 + 8 + 8 + 16 + 1 + 1, // 32+64+64+8*16+8+8
            MediaDevicePath::FilePath(FilePathDevicePath { path_name }) => path_name.encode_utf16().chain([0x0000]).count() as u16 * 2,
        }
    }

    pub fn sub_type(&self) -> u8 {
        match self {
            MediaDevicePath::HardDrive(_) => Self::HARD_DRIVE_SUBTYPE,
            MediaDevicePath::FilePath(_) => Self::FILEPATH_SUBTYPE,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HardDriveDevicePath {
    partition_number: u32,
    partition_start: u64,
    partition_size: u64,
    signature: Signature,
    partition_table: PartitionTableType,
}

impl HardDriveDevicePath {
    pub fn new_gpt(partition_number: u32, partition_start: u64, partition_size: u64, uuid: Uuid) -> Self {
        HardDriveDevicePath {
            partition_number,
            partition_start,
            partition_size,
            signature: Signature::GUID(uuid),
            partition_table: PartitionTableType::GPT,
        }
    }

    pub fn parse(read: &mut impl Read) -> Result<Self> {
        use DevicePathProtocolParseError::ParseSubType;

        let partition_number = read.read_u32::<LittleEndian>()?;
        let partition_start = read.read_u64::<LittleEndian>()?;
        let partition_size = read.read_u64::<LittleEndian>()?;
        let signature_data = {
            let mut buffer = [0u8; 16];
            read.read_exact(&mut buffer)?;
            buffer
        };
        let partition_table = read.read_u8()?.try_into()
            .map_err(|err| ParseSubType { sub_type: "HardDriveDevicePath".to_owned(), message: "parse partition table".to_owned(), source: Some(Box::new(err)) })?;
        let signature_type = read.read_u8()?.try_into()
            .map_err(|err| ParseSubType { sub_type: "HardDriveDevicePath".to_owned(), message: "parse signature type".to_owned(), source: Some(Box::new(err)) })?;
        let signature = match signature_type {
            Signature::NO_SIGNATURE => Signature::None(signature_data),
            Signature::MBR_SIGNATURE => Signature::MBRSignature(signature_data),
            Signature::GUID_SIGNATURE => Signature::GUID(Uuid::from_bytes_le(signature_data)),
            _ => return Err(ParseSubType { sub_type: "HardDriveDevicePath".to_owned(), message: "unknown signature type".to_owned(), source: None }),
        };

        Ok(HardDriveDevicePath {
            partition_number,
            partition_start,
            partition_size,
            signature,
            partition_table,
        })
    }

    pub fn write(&self, write: &mut impl Write) -> io::Result<()> {
        write.write_u32::<LittleEndian>(self.partition_number)?;
        write.write_u64::<LittleEndian>(self.partition_start)?;
        write.write_u64::<LittleEndian>(self.partition_size)?;

        match self.signature {
            Signature::None(data) | Signature::MBRSignature(data) => write.write_all(&data)?,
            Signature::GUID(uuid) => write.write_all(uuid.to_bytes_le().as_slice())?,
        };

        write.write_u8(self.partition_table as u8)?;
        write.write_u8(u8::from(&self.signature))?;

        Ok(())
    }
}

#[derive(Copy, Clone, Debug, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum PartitionTableType {
    MBR = 0x01,
    GPT = 0x02,
}

#[derive(Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum Signature {
    None([u8; 16]) = Signature::NO_SIGNATURE,
    /// 32-bit signature from address 0x1b8 of the type 0x01 MBR.
    MBRSignature([u8; 16]) = Signature::MBR_SIGNATURE,
    GUID(Uuid) = Signature::GUID_SIGNATURE,
}

impl From<&Signature> for u8 {
    fn from(value: &Signature) -> Self {
        match value {
            Signature::None(_) => Signature::NO_SIGNATURE,
            Signature::MBRSignature(_) => Signature::MBR_SIGNATURE,
            Signature::GUID(_) => Signature::GUID_SIGNATURE,
        }
    }
}

impl Signature {
    const NO_SIGNATURE: u8 = 0x00;
    const MBR_SIGNATURE: u8 = 0x01;
    const GUID_SIGNATURE: u8 = 0x02;
}

#[derive(Clone, Debug, PartialEq)]
pub struct FilePathDevicePath {
    path_name: String,
}

impl FilePathDevicePath {
    pub fn parse(read: &mut impl Read) -> Result<Self> {
        use DevicePathProtocolParseError::ParseSubType;

        let mut buffer = vec![];
        loop {
            let char = read.read_u16::<LittleEndian>()?;
            if char == 0x0000 {
                break;
            }
            buffer.push(char);
        }
        let path_name = String::from_utf16(&buffer)
            .map_err(|err| ParseSubType { sub_type: "FilePathDevicePath".to_owned(), message: "parse utf-16".to_owned(), source: Some(Box::new(err)) })?;
        Ok(FilePathDevicePath { path_name })
    }
}

#[derive(Clone, Debug, PartialEq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum EndSubType {
    EndEntireDevicePath = EndSubType::END_ENTIRE_DEVICE_PATH,
    EndInstanceDevicePath = EndSubType::END_INSTANCE_DEVICE_PATH,
}

impl EndSubType {
    const END_INSTANCE_DEVICE_PATH: u8 = 0x01;
    const END_ENTIRE_DEVICE_PATH: u8 = 0xFF;

    pub fn sub_type(&self) -> u8 {
        match self {
            EndSubType::EndEntireDevicePath => Self::END_ENTIRE_DEVICE_PATH,
            EndSubType::EndInstanceDevicePath => Self::END_INSTANCE_DEVICE_PATH,
        }
    }
}
