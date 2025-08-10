use rusb::{Context, DeviceHandle, Direction, TransferType, UsbContext};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ChdkPtpError {
    #[error("USB error: {0}")]
    UsbError(#[from] rusb::Error),
    #[error("Device not found")]
    DeviceNotFound,
    #[error("Interface not found")]
    InterfaceNotFound,
    #[error("Endpoint not found")]
    EndpointNotFound,
    #[error("PTP command failed: {0}")]
    PtpCommandFailed(String),
}

/// CHDK PTP protocol constants
pub const PTP_OC_CHDK: u16 = 0x9999;
pub const PTP_CHDK_VERSION_MAJOR: u16 = 2;
pub const PTP_CHDK_VERSION_MINOR: u16 = 9;

/// CHDK PTP commands
#[repr(u16)]
pub enum PtpChdkCommand {
    Version = 0,
    GetMemory = 1,
    SetMemory = 2,
    CallFunction = 3,
    TempData = 4,
    UploadFile = 5,
    DownloadFile = 6,
    ExecuteScript = 7,
    ScriptStatus = 8,
    ScriptSupport = 9,
    ReadScriptMsg = 10,
    WriteScriptMsg = 11,
    GetDisplayData = 12,
    RemoteCaptureIsReady = 13,
    RemoteCaptureGetData = 14,
}

/// PTP Response Code
#[repr(u16)]
pub enum PtpResponseCode {
    OK = 0x2001,
    GeneralError = 0x2002,
    SessionNotOpen = 0x2003,
    InvalidTransactionID = 0x2004,
    OperationNotSupported = 0x2005,
    ParameterNotSupported = 0x2006,
    IncompleteTransfer = 0x2007,
    InvalidStorageID = 0x2008,
    InvalidObjectHandle = 0x2009,
    DevicePropNotSupported = 0x200A,
    InvalidObjectFormatCode = 0x200B,
    StoreFull = 0x200C,
    ObjectWriteProtected = 0x200D,
    StoreReadOnly = 0x200E,
    AccessDenied = 0x200F,
    NoThumbnailPresent = 0x2010,
    SelfTestFailed = 0x2011,
    PartialDeletion = 0x2012,
    StoreNotAvailable = 0x2013,
    SpecificationByFormatUnsupported = 0x2014,
    NoValidObjectInfo = 0x2015,
    InvalidCodeFormat = 0x2016,
    UnknownVendorCode = 0x2017,
    CaptureAlreadyTerminated = 0x2018,
    DeviceBusy = 0x2019,
    InvalidParentObject = 0x201A,
    InvalidDevicePropFormat = 0x201B,
    InvalidDevicePropValue = 0x201C,
    InvalidParameter = 0x201D,
    SessionAlreadyOpen = 0x201E,
    TransactionCancelled = 0x201F,
    SpecificationOfDestinationUnsupported = 0x2020,
}

/// PTP Container structure
#[derive(Debug, Clone)]
pub struct PtpContainer {
    pub length: u32,
    pub type_code: u16,
    pub code: u16,
    pub transaction_id: u32,
    pub payload: Vec<u8>,
}

impl PtpContainer {
    pub fn new(type_code: u16, code: u16, transaction_id: u32, payload: Vec<u8>) -> Self {
        let length = 12 + payload.len() as u32; // 12 bytes header + payload
        Self {
            length,
            type_code,
            code,
            transaction_id,
            payload,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.length.to_le_bytes());
        bytes.extend_from_slice(&self.type_code.to_le_bytes());
        bytes.extend_from_slice(&self.code.to_le_bytes());
        bytes.extend_from_slice(&self.transaction_id.to_le_bytes());
        bytes.extend_from_slice(&self.payload);
        bytes
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, ChdkPtpError> {
        if data.len() < 12 {
            return Err(ChdkPtpError::PtpCommandFailed("Invalid container size".to_string()));
        }

        let length = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let type_code = u16::from_le_bytes([data[4], data[5]]);
        let code = u16::from_le_bytes([data[6], data[7]]);
        let transaction_id = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let payload = data[12..].to_vec();

        Ok(Self {
            length,
            type_code,
            code,
            transaction_id,
            payload,
        })
    }
}

/// CHDK PTP Client
pub struct ChdkPtpClient {
    handle: DeviceHandle<Context>,
    transaction_id: u32,
}

impl ChdkPtpClient {
    /// Create a new CHDK PTP client
    pub fn new(vendor_id: u16, product_id: u16) -> Result<Self, ChdkPtpError> {
        let context = Context::new()?;
        let devices = context.devices()?;
        
        let device = devices
            .iter()
            .find(|device| {
                if let Ok(desc) = device.device_descriptor() {
                    desc.vendor_id() == vendor_id && desc.product_id() == product_id
                } else {
                    false
                }
            })
            .ok_or(ChdkPtpError::DeviceNotFound)?;

        let handle = device.open()?;
        
        // Find PTP interface
        let config_desc = device.config_descriptor(0)?;
        let ptp_interface = config_desc
            .interfaces()
            .find(|interface| {
                interface.descriptors().any(|desc| {
                    desc.class_code() == 0xFF && desc.sub_class_code() == 0xFF
                })
            })
            .ok_or(ChdkPtpError::InterfaceNotFound)?;

        let interface_number = ptp_interface.number();
        
        // Claim the interface
        handle.claim_interface(interface_number)?;
        
        Ok(Self {
            handle,
            transaction_id: 1,
        })
    }

    /// Send a PTP command and receive response
    fn send_command(&mut self, code: u16, param1: u32, param2: u32, param3: u32, data: Vec<u8>) -> Result<PtpContainer, ChdkPtpError> {
        // Find bulk out endpoint
        let config_desc = self.handle.device().config_descriptor(0)?;
        let ptp_interface = config_desc
            .interfaces()
            .find(|interface| {
                interface.descriptors().any(|desc| {
                    desc.class_code() == 0xFF && desc.sub_class_code() == 0xFF
                })
            })
            .ok_or(ChdkPtpError::InterfaceNotFound)?;

        let bulk_out_endpoint = ptp_interface
            .descriptors()
            .flat_map(|desc| desc.endpoint_descriptors())
            .find(|ep| ep.direction() == Direction::Out && ep.transfer_type() == TransferType::Bulk)
            .ok_or(ChdkPtpError::EndpointNotFound)?;

        let bulk_in_endpoint = ptp_interface
            .descriptors()
            .flat_map(|desc| desc.endpoint_descriptors())
            .find(|ep| ep.direction() == Direction::In && ep.transfer_type() == TransferType::Bulk)
            .ok_or(ChdkPtpError::EndpointNotFound)?;

        // Create command container
        let mut payload = Vec::new();
        payload.extend_from_slice(&param1.to_le_bytes());
        payload.extend_from_slice(&param2.to_le_bytes());
        payload.extend_from_slice(&param3.to_le_bytes());
        payload.extend_from_slice(&data);

        let container = PtpContainer::new(0x0001, code, self.transaction_id, payload);
        let container_bytes = container.to_bytes();

        // Send command
        let timeout = std::time::Duration::from_secs(5);
        let bytes_written = self.handle.write_bulk(
            bulk_out_endpoint.address(),
            &container_bytes,
            timeout,
        )?;

        if bytes_written != container_bytes.len() {
            return Err(ChdkPtpError::PtpCommandFailed("Incomplete write".to_string()));
        }

        // Read response
        let mut response_buffer = vec![0u8; 512];
        let bytes_read = self.handle.read_bulk(
            bulk_in_endpoint.address(),
            &mut response_buffer,
            timeout,
        )?;

        response_buffer.truncate(bytes_read);
        let response = PtpContainer::from_bytes(&response_buffer)?;

        // Check response code
        if response.code != PtpResponseCode::OK as u16 {
            return Err(ChdkPtpError::PtpCommandFailed(format!("PTP error: 0x{:04X}", response.code)));
        }

        self.transaction_id = self.transaction_id.wrapping_add(1);
        Ok(response)
    }

    /// Get CHDK version (most basic command)
    pub fn get_version(&mut self) -> Result<(u16, u16), ChdkPtpError> {
        let response = self.send_command(
            PTP_OC_CHDK,
            PtpChdkCommand::Version as u32,
            0,
            0,
            Vec::new(),
        )?;

        if response.payload.len() >= 4 {
            let major = u16::from_le_bytes([response.payload[0], response.payload[1]]);
            let minor = u16::from_le_bytes([response.payload[2], response.payload[3]]);
            Ok((major, minor))
        } else {
            Err(ChdkPtpError::PtpCommandFailed("Invalid version response".to_string()))
        }
    }

    /// Example method showing how to add more CHDK PTP commands
    /// This demonstrates the pattern for implementing additional commands
    pub fn get_memory(&mut self, address: u32, size: u32, mode: u32) -> Result<Vec<u8>, ChdkPtpError> {
        let response = self.send_command(
            PTP_OC_CHDK,
            PtpChdkCommand::GetMemory as u32,
            address,  // param2: base address
            size,     // param3: size in bytes
            vec![mode as u8], // data: mode as payload
        )?;

        // The response payload contains the memory data
        Ok(response.payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ptp_container_serialization() {
        let payload = vec![1, 2, 3, 4];
        let container = PtpContainer::new(0x0001, 0x9999, 1, payload.clone());
        let bytes = container.to_bytes();
        let deserialized = PtpContainer::from_bytes(&bytes).unwrap();
        
        assert_eq!(deserialized.type_code, container.type_code);
        assert_eq!(deserialized.code, container.code);
        assert_eq!(deserialized.transaction_id, container.transaction_id);
        assert_eq!(deserialized.payload, container.payload);
    }
}
