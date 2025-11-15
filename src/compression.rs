use std::io::Error as IoError;
use tracing::debug;

/// Trait for compression algorithms
pub trait Decompressor {
    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, IoError>;
    fn can_handle(&self, data: &[u8]) -> bool;
}

/// GZIP decompressor
pub struct GzipDecompressor;

impl Decompressor for GzipDecompressor {
    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, IoError> {
        use flate2::read::GzDecoder;
        use std::io::Read;

        debug!("Starting GZIP decompression for {} bytes", data.len());
        let mut decoder = GzDecoder::new(data);
        let mut decompressed = Vec::new();

        match decoder.read_to_end(&mut decompressed) {
            Ok(bytes_read) => {
                debug!("GZIP decompression successful: {} bytes read", bytes_read);
                Ok(decompressed)
            }
            Err(e) => {
                debug!("GZIP decompression failed: {:?}", e);
                Err(e)
            }
        }
    }

    fn can_handle(&self, data: &[u8]) -> bool {
        data.len() > 2 && data[0] == 0x1f && data[1] == 0x8b
    }
}

/// ZLIB decompressor
pub struct ZlibDecompressor;

impl Decompressor for ZlibDecompressor {
    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, IoError> {
        use flate2::read::ZlibDecoder;
        use std::io::Read;

        debug!("Starting ZLIB decompression for {} bytes", data.len());
        let mut decoder = ZlibDecoder::new(data);
        let mut decompressed = Vec::new();

        match decoder.read_to_end(&mut decompressed) {
            Ok(bytes_read) => {
                debug!("ZLIB decompression successful: {} bytes read", bytes_read);
                Ok(decompressed)
            }
            Err(e) => {
                debug!("ZLIB decompression failed: {:?}", e);
                Err(e)
            }
        }
    }

    fn can_handle(&self, data: &[u8]) -> bool {
        data.len() > 2 && data[0] == 0x78 && (data[1] == 0x9c || data[1] == 0xda || data[1] == 0x01)
    }
}

/// Compression manager that handles multiple decompression algorithms
pub struct CompressionManager {
    decompressors: Vec<Box<dyn Decompressor + Send + Sync>>,
}

impl CompressionManager {
    pub fn new() -> Self {
        let decompressors: Vec<Box<dyn Decompressor + Send + Sync>> = vec![
            Box::new(GzipDecompressor),
            Box::new(ZlibDecompressor),
        ];
        
        Self { decompressors }
    }

    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, IoError> {
        for decompressor in &self.decompressors {
            if decompressor.can_handle(data) {
                let compression_type = if decompressor.can_handle(data) {
                    if data[0] == 0x1f && data[1] == 0x8b { "GZIP" } else { "ZLIB" }
                } else { "unknown" };
                
                debug!("Message compression detected: {}", compression_type);
                return decompressor.decompress(data);
            }
        }
        
        debug!("No compression detected, returning original data");
        Ok(data.to_vec())
    }
}

impl Default for CompressionManager {
    fn default() -> Self {
        Self::new()
    }
}