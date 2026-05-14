//! Stratum protocol implementation for Bitcoin mining pools
//!
//! This module implements the Stratum mining protocol (v1) for communication
//! with Bitcoin mining pools.

use crate::{PoolClient, PoolConfig, PoolError, Result, Share};
use bminer_core::Work;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

/// Stratum JSON-RPC request
#[derive(Debug, Serialize)]
struct StratumRequest {
    id: u64,
    method: String,
    params: Vec<Value>,
}

/// Stratum JSON-RPC response
#[derive(Debug, Deserialize)]
struct StratumResponse {
    id: Option<u64>,
    result: Option<Value>,
    error: Option<Value>,
    method: Option<String>,
    params: Option<Vec<Value>>,
}

/// Stratum client implementation
pub struct StratumClient {
    config: PoolConfig,
    writer: Option<Mutex<WriteHalf<TcpStream>>>,
    reader: Option<Mutex<BufReader<ReadHalf<TcpStream>>>>,
    request_id: AtomicU64,
    extranonce1: Vec<u8>,
    extranonce2_size: usize,
    difficulty: f64,
    current_work: Option<Work>,
}

impl StratumClient {
    /// Create a new Stratum client
    pub fn new(config: PoolConfig) -> Self {
        Self {
            config,
            writer: None,
            reader: None,
            request_id: AtomicU64::new(1),
            extranonce1: Vec::new(),
            extranonce2_size: 0,
            difficulty: 1.0,
            current_work: None,
        }
    }
    
    /// Get next request ID
    fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Build the pool worker identity used for authorization and share submission.
    fn worker_identity(&self) -> String {
        let username = self.config.username.trim();
        let worker_name = self.config.worker_name.trim();

        if worker_name.is_empty() {
            return username.to_string();
        }

        if username
            .rsplit_once('.')
            .is_some_and(|(_, suffix)| suffix == worker_name)
        {
            return username.to_string();
        }

        format!("{}.{}", username, worker_name)
    }
    
    /// Send a JSON-RPC request
    async fn send_request(&self, method: &str, params: Vec<Value>) -> Result<()> {
        let request = StratumRequest {
            id: self.next_id(),
            method: method.to_string(),
            params,
        };
        
        let json = serde_json::to_string(&request)
            .map_err(|e| PoolError::ProtocolError(format!("Failed to serialize request: {}", e)))?;
        
        if let Some(writer) = &self.writer {
            let mut writer = writer.lock().await;
            writer.write_all(json.as_bytes()).await
                .map_err(|e| PoolError::ConnectionError(e.to_string()))?;
            writer.write_all(b"\n").await
                .map_err(|e| PoolError::ConnectionError(e.to_string()))?;
            writer.flush().await
                .map_err(|e| PoolError::ConnectionError(e.to_string()))?;
        } else {
            return Err(PoolError::ConnectionError("Not connected".to_string()));
        }
        
        Ok(())
    }
    
    /// Receive a JSON-RPC response
    async fn receive_response(&self) -> Result<StratumResponse> {
        if let Some(reader) = &self.reader {
            let mut reader = reader.lock().await;
            let mut line = String::new();
            
            reader.read_line(&mut line).await
                .map_err(|e| PoolError::ConnectionError(e.to_string()))?;
            
            if line.is_empty() {
                return Err(PoolError::ConnectionError("Connection closed".to_string()));
            }
            
            serde_json::from_str(&line)
                .map_err(|e| PoolError::ProtocolError(format!("Failed to parse response: {}", e)))
        } else {
            Err(PoolError::ConnectionError("Not connected".to_string()))
        }
    }
    
    /// Parse mining.notify parameters into Work
    fn parse_work(&self, params: &[Value]) -> Result<Work> {
        if params.len() < 9 {
            return Err(PoolError::ProtocolError("Invalid mining.notify params".to_string()));
        }
        
        let job_id = params[0].as_str()
            .ok_or_else(|| PoolError::ProtocolError("Invalid job_id".to_string()))?
            .to_string();
        
        let prev_hash_hex = params[1].as_str()
            .ok_or_else(|| PoolError::ProtocolError("Invalid prevhash".to_string()))?;
        let prev_hash = hex::decode(prev_hash_hex)
            .map_err(|e| PoolError::ProtocolError(format!("Invalid prevhash hex: {}", e)))?;
        let mut prev_hash_array = [0u8; 32];
        prev_hash_array.copy_from_slice(&prev_hash);
        
        let coinb1_hex = params[2].as_str()
            .ok_or_else(|| PoolError::ProtocolError("Invalid coinb1".to_string()))?;
        let coinb2_hex = params[3].as_str()
            .ok_or_else(|| PoolError::ProtocolError("Invalid coinb2".to_string()))?;
        
        let mut coinbase = hex::decode(coinb1_hex)
            .map_err(|e| PoolError::ProtocolError(format!("Invalid coinb1 hex: {}", e)))?;
        let coinb2 = hex::decode(coinb2_hex)
            .map_err(|e| PoolError::ProtocolError(format!("Invalid coinb2 hex: {}", e)))?;
        coinbase.extend_from_slice(&coinb2);
        
        let merkle_branches: Vec<[u8; 32]> = params[4].as_array()
            .ok_or_else(|| PoolError::ProtocolError("Invalid merkle_branch".to_string()))?
            .iter()
            .map(|v| {
                let hex_str = v.as_str()
                    .ok_or_else(|| PoolError::ProtocolError("Invalid merkle branch".to_string()))?;
                let bytes = hex::decode(hex_str)
                    .map_err(|e| PoolError::ProtocolError(format!("Invalid merkle hex: {}", e)))?;
                let mut array = [0u8; 32];
                array.copy_from_slice(&bytes);
                Ok(array)
            })
            .collect::<Result<Vec<_>>>()?;
        
        let version = params[5].as_str()
            .ok_or_else(|| PoolError::ProtocolError("Invalid version".to_string()))?;
        let version = u32::from_str_radix(version, 16)
            .map_err(|e| PoolError::ProtocolError(format!("Invalid version hex: {}", e)))?;
        
        let nbits = params[6].as_str()
            .ok_or_else(|| PoolError::ProtocolError("Invalid nbits".to_string()))?;
        let nbits = u32::from_str_radix(nbits, 16)
            .map_err(|e| PoolError::ProtocolError(format!("Invalid nbits hex: {}", e)))?;
        
        let ntime = params[7].as_str()
            .ok_or_else(|| PoolError::ProtocolError("Invalid ntime".to_string()))?;
        let ntime = u32::from_str_radix(ntime, 16)
            .map_err(|e| PoolError::ProtocolError(format!("Invalid ntime hex: {}", e)))?;
        
        let mut work = Work::new(
            job_id,
            prev_hash_array,
            coinbase,
            merkle_branches,
            version,
            nbits,
            ntime,
        );
        
        work.extranonce1 = self.extranonce1.clone();
        work.extranonce2_size = self.extranonce2_size;
        
        Ok(work)
    }
}

#[async_trait::async_trait]
impl PoolClient for StratumClient {
    async fn connect(&mut self) -> Result<()> {
        // Parse URL to extract host and port
        let url = self.config.url.trim_start_matches("stratum+tcp://");
        
        tracing::info!("Connecting to pool: {}", url);
        
        let stream = TcpStream::connect(url).await
            .map_err(|e| PoolError::ConnectionError(format!("Failed to connect: {}", e)))?;
        
        // Split stream into reader and writer
        let (read_half, write_half) = tokio::io::split(stream);
        
        self.writer = Some(Mutex::new(write_half));
        self.reader = Some(Mutex::new(BufReader::new(read_half)));
        
        tracing::info!("Connected to pool");
        Ok(())
    }
    
    async fn subscribe(&mut self) -> Result<()> {
        tracing::info!("Subscribing to pool");
        
        self.send_request("mining.subscribe", vec![
            json!("bminer/0.1.0"),
        ]).await?;
        
        let response = self.receive_response().await?;
        
        if let Some(error) = response.error {
            return Err(PoolError::ProtocolError(format!("Subscribe failed: {:?}", error)));
        }
        
        if let Some(result) = response.result {
            if let Some(array) = result.as_array() {
                if array.len() >= 2 {
                    // Extract extranonce1 and extranonce2_size
                    if let Some(extranonce1_hex) = array[1].as_str() {
                        self.extranonce1 = hex::decode(extranonce1_hex)
                            .map_err(|e| PoolError::ProtocolError(format!("Invalid extranonce1: {}", e)))?;
                    }
                    
                    if let Some(size) = array[2].as_u64() {
                        self.extranonce2_size = size as usize;
                    }
                }
            }
        }
        
        tracing::info!("Subscribed successfully");
        Ok(())
    }
    
    async fn authorize(&mut self) -> Result<()> {
        let worker_identity = self.worker_identity();
        tracing::info!("Authorizing worker: {}", worker_identity);

        self.send_request("mining.authorize", vec![
            json!(worker_identity),
            json!(self.config.password),
        ]).await?;
        
        let response = self.receive_response().await?;
        
        if let Some(error) = response.error {
            return Err(PoolError::AuthenticationFailed);
        }
        
        if let Some(result) = response.result {
            if !result.as_bool().unwrap_or(false) {
                return Err(PoolError::AuthenticationFailed);
            }
        }
        
        tracing::info!("Authorized successfully");
        Ok(())
    }
    
    async fn receive_work(&mut self) -> Result<Work> {
        loop {
            let response = self.receive_response().await?;
            
            // Check if this is a mining.notify message
            if let Some(method) = response.method {
                if method == "mining.notify" {
                    if let Some(params) = response.params {
                        let work = self.parse_work(&params)?;
                        self.current_work = Some(work.clone());
                        return Ok(work);
                    }
                } else if method == "mining.set_difficulty" {
                    if let Some(params) = response.params {
                        if let Some(diff) = params.get(0).and_then(|v| v.as_f64()) {
                            self.difficulty = diff;
                            tracing::info!("Difficulty set to: {}", diff);
                        }
                    }
                }
            }
        }
    }
    
    async fn submit_share(&mut self, share: Share) -> Result<bool> {
        tracing::info!("Submitting share for job: {}", share.job_id);

        let extranonce2_hex = hex::encode(&share.extranonce2);
        let ntime_hex = format!("{:08x}", share.ntime);
        let nonce_hex = format!("{:08x}", share.nonce);
        let worker_identity = self.worker_identity();

        self.send_request("mining.submit", vec![
            json!(worker_identity),
            json!(share.job_id),
            json!(extranonce2_hex),
            json!(ntime_hex),
            json!(nonce_hex),
        ]).await?;
        
        let response = self.receive_response().await?;
        
        if let Some(error) = response.error {
            tracing::warn!("Share rejected: {:?}", error);
            return Ok(false);
        }
        
        if let Some(result) = response.result {
            let accepted = result.as_bool().unwrap_or(false);
            if accepted {
                tracing::info!("Share accepted!");
            } else {
                tracing::warn!("Share rejected");
            }
            return Ok(accepted);
        }
        
        Ok(false)
    }
    
    async fn disconnect(&mut self) -> Result<()> {
        self.writer = None;
        self.reader = None;
        tracing::info!("Disconnected from pool");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::StratumClient;
    use crate::PoolConfig;

    fn test_client(username: &str, worker_name: &str) -> StratumClient {
        StratumClient::new(PoolConfig {
            url: "stratum+tcp://pool.example.com:3333".to_string(),
            username: username.to_string(),
            password: "x".to_string(),
            worker_name: worker_name.to_string(),
        })
    }

    #[test]
    fn appends_worker_name_to_wallet_address() {
        let client = test_client(
            "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh",
            "bminer",
        );

        assert_eq!(
            client.worker_identity(),
            "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh.bminer"
        );
    }

    #[test]
    fn preserves_prequalified_username() {
        let client = test_client("wallet.worker1", "worker1");

        assert_eq!(client.worker_identity(), "wallet.worker1");
    }

    #[test]
    fn leaves_username_unchanged_without_worker_name() {
        let client = test_client("wallet-only", "");

        assert_eq!(client.worker_identity(), "wallet-only");
    }
}

// Made with Bob
