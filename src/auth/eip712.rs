// EIP-712 order signing for Polymarket CTFExchange.
//
// Domain: name="ClobAuthDomain", version="1", chainId=137
// CTFExchange: 0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E
// NegRiskExchange: 0xC5d563A36AE78145C45a50134d48A1215220f80a
//
// The order struct is hashed per EIP-712 and signed with the wallet's private key.
// Polymarket verifies this signature on-chain when the order fills.

use ethers::core::types::{Address, Signature, H256, U256};
use ethers::signers::LocalWallet;
use ethers::utils::keccak256;
use std::str::FromStr;

use crate::clob::types::OrderData;
use crate::error::{AppError, Result};

/// CTFExchange contract address on Polygon.
pub const CTF_EXCHANGE: &str = "0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E";
/// NegRiskExchange contract address on Polygon.
pub const NEG_RISK_EXCHANGE: &str = "0xC5d563A36AE78145C45a50134d48A1215220f80a";
/// Polygon chain ID.
pub const CHAIN_ID: u64 = 137;

/// EIP-712 domain separator hash.
/// keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)")
fn domain_separator_typehash() -> [u8; 32] {
    keccak256(b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)")
}

/// EIP-712 order struct typehash.
/// keccak256("Order(uint256 salt,address maker,address signer,address taker,uint256 tokenId,uint256 makerAmount,uint256 takerAmount,uint256 expiration,uint256 nonce,uint256 feeRateBps,uint8 side,uint8 signatureType)")
fn order_typehash() -> [u8; 32] {
    keccak256(b"Order(uint256 salt,address maker,address signer,address taker,uint256 tokenId,uint256 makerAmount,uint256 takerAmount,uint256 expiration,uint256 nonce,uint256 feeRateBps,uint8 side,uint8 signatureType)")
}

/// Compute the EIP-712 domain separator for a given exchange contract.
pub fn compute_domain_separator(exchange_address: &str) -> Result<[u8; 32]> {
    let addr = Address::from_str(exchange_address)
        .map_err(|e| AppError::Auth(format!("invalid exchange address: {}", e)))?;

    let mut encoded = Vec::with_capacity(160);
    encoded.extend_from_slice(&domain_separator_typehash());
    encoded.extend_from_slice(&keccak256(b"ClobAuthDomain"));
    encoded.extend_from_slice(&keccak256(b"1"));
    // chainId as uint256 (32 bytes, big-endian)
    let mut chain_id_bytes = [0u8; 32];
    U256::from(CHAIN_ID).to_big_endian(&mut chain_id_bytes);
    encoded.extend_from_slice(&chain_id_bytes);
    // address as uint256 (left-padded to 32 bytes)
    let mut addr_bytes = [0u8; 32];
    addr_bytes[12..].copy_from_slice(addr.as_bytes());
    encoded.extend_from_slice(&addr_bytes);

    Ok(keccak256(&encoded))
}

/// Encode an order struct per EIP-712 and return its hash.
pub fn hash_order(order: &OrderData) -> Result<[u8; 32]> {
    let maker = Address::from_str(&order.maker)
        .map_err(|e| AppError::Auth(format!("invalid maker address: {}", e)))?;
    let signer_addr = Address::from_str(&order.signer)
        .map_err(|e| AppError::Auth(format!("invalid signer address: {}", e)))?;

    let salt = U256::from_dec_str(&order.salt)
        .map_err(|e| AppError::Auth(format!("invalid salt: {}", e)))?;
    let token_id = U256::from_dec_str(&order.token_id)
        .map_err(|e| AppError::Auth(format!("invalid token_id: {}", e)))?;
    let maker_amount = U256::from_dec_str(&order.maker_amount)
        .map_err(|e| AppError::Auth(format!("invalid maker_amount: {}", e)))?;
    let taker_amount = U256::from_dec_str(&order.taker_amount)
        .map_err(|e| AppError::Auth(format!("invalid taker_amount: {}", e)))?;
    let expiration = U256::from_dec_str(&order.expiration)
        .map_err(|e| AppError::Auth(format!("invalid expiration: {}", e)))?;
    let nonce = U256::from_dec_str(&order.nonce)
        .map_err(|e| AppError::Auth(format!("invalid nonce: {}", e)))?;
    let fee_rate_bps = U256::from_dec_str(&order.fee_rate_bps)
        .map_err(|e| AppError::Auth(format!("invalid fee_rate_bps: {}", e)))?;
    let side = U256::from(order.side.parse::<u8>()
        .map_err(|e| AppError::Auth(format!("invalid side: {}", e)))?);
    let sig_type = U256::from(order.signature_type);

    // taker is always address(0) for CLOB orders
    let taker = Address::zero();

    let mut encoded = Vec::with_capacity(384);
    encoded.extend_from_slice(&order_typehash());

    // Each field is abi-encoded as uint256 (32 bytes)
    let mut buf = [0u8; 32];

    salt.to_big_endian(&mut buf);
    encoded.extend_from_slice(&buf);

    // maker address
    let mut addr_buf = [0u8; 32];
    addr_buf[12..].copy_from_slice(maker.as_bytes());
    encoded.extend_from_slice(&addr_buf);

    // signer address
    addr_buf = [0u8; 32];
    addr_buf[12..].copy_from_slice(signer_addr.as_bytes());
    encoded.extend_from_slice(&addr_buf);

    // taker address (zero)
    addr_buf = [0u8; 32];
    addr_buf[12..].copy_from_slice(taker.as_bytes());
    encoded.extend_from_slice(&addr_buf);

    token_id.to_big_endian(&mut buf);
    encoded.extend_from_slice(&buf);

    maker_amount.to_big_endian(&mut buf);
    encoded.extend_from_slice(&buf);

    taker_amount.to_big_endian(&mut buf);
    encoded.extend_from_slice(&buf);

    expiration.to_big_endian(&mut buf);
    encoded.extend_from_slice(&buf);

    nonce.to_big_endian(&mut buf);
    encoded.extend_from_slice(&buf);

    fee_rate_bps.to_big_endian(&mut buf);
    encoded.extend_from_slice(&buf);

    side.to_big_endian(&mut buf);
    encoded.extend_from_slice(&buf);

    sig_type.to_big_endian(&mut buf);
    encoded.extend_from_slice(&buf);

    Ok(keccak256(&encoded))
}

/// Build the final EIP-712 digest: keccak256("\x19\x01" || domainSeparator || structHash)
pub fn eip712_digest(domain_separator: &[u8; 32], struct_hash: &[u8; 32]) -> [u8; 32] {
    let mut msg = Vec::with_capacity(66);
    msg.push(0x19);
    msg.push(0x01);
    msg.extend_from_slice(domain_separator);
    msg.extend_from_slice(struct_hash);
    keccak256(&msg)
}

/// Sign an order with the given wallet. Returns the signature hex string.
pub async fn sign_order(
    wallet: &LocalWallet,
    order: &OrderData,
    is_neg_risk: bool,
) -> Result<String> {
    let exchange = if is_neg_risk { NEG_RISK_EXCHANGE } else { CTF_EXCHANGE };
    let domain_sep = compute_domain_separator(exchange)?;
    let struct_hash = hash_order(order)?;
    let digest = eip712_digest(&domain_sep, &struct_hash);

    let signature: Signature = wallet
        .sign_hash(H256::from(digest))
        .map_err(|e| AppError::Auth(format!("signing failed: {}", e)))?;

    Ok(format!("0x{}", hex::encode(signature.to_vec())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethers::signers::LocalWallet;

    fn test_wallet() -> LocalWallet {
        // Deterministic test wallet -- never use in production
        "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
            .parse::<LocalWallet>()
            .unwrap()
            .with_chain_id(CHAIN_ID)
    }

    fn test_order() -> OrderData {
        let wallet = test_wallet();
        let addr = format!("{:?}", wallet.address());
        OrderData {
            salt: "123456789".to_string(),
            maker: addr.clone(),
            signer: addr,
            token_id: "1234567890".to_string(),
            maker_amount: "10000000".to_string(),
            taker_amount: "5000000".to_string(),
            side: "0".to_string(),
            expiration: "0".to_string(),
            nonce: "0".to_string(),
            fee_rate_bps: "0".to_string(),
            signature_type: 0,
        }
    }

    #[test]
    fn test_domain_separator_deterministic() {
        let ds1 = compute_domain_separator(CTF_EXCHANGE).unwrap();
        let ds2 = compute_domain_separator(CTF_EXCHANGE).unwrap();
        assert_eq!(ds1, ds2);
    }

    #[test]
    fn test_domain_separator_differs_by_exchange() {
        let ds_ctf = compute_domain_separator(CTF_EXCHANGE).unwrap();
        let ds_neg = compute_domain_separator(NEG_RISK_EXCHANGE).unwrap();
        assert_ne!(ds_ctf, ds_neg);
    }

    #[test]
    fn test_domain_separator_invalid_address() {
        let result = compute_domain_separator("not-an-address");
        assert!(result.is_err());
    }

    #[test]
    fn test_hash_order_deterministic() {
        let order = test_order();
        let h1 = hash_order(&order).unwrap();
        let h2 = hash_order(&order).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_order_differs_by_salt() {
        let mut o1 = test_order();
        let mut o2 = test_order();
        o1.salt = "111".to_string();
        o2.salt = "222".to_string();
        assert_ne!(hash_order(&o1).unwrap(), hash_order(&o2).unwrap());
    }

    #[test]
    fn test_eip712_digest_format() {
        let ds = compute_domain_separator(CTF_EXCHANGE).unwrap();
        let order = test_order();
        let sh = hash_order(&order).unwrap();
        let digest = eip712_digest(&ds, &sh);
        assert_eq!(digest.len(), 32);
    }

    #[tokio::test]
    async fn test_sign_order_produces_valid_signature() {
        let wallet = test_wallet();
        let order = test_order();
        let sig = sign_order(&wallet, &order, false).await.unwrap();

        // Signature should be 0x-prefixed, 65 bytes = 130 hex chars + 2 for "0x"
        assert!(sig.starts_with("0x"));
        assert_eq!(sig.len(), 132);
    }

    #[tokio::test]
    async fn test_sign_order_deterministic() {
        let wallet = test_wallet();
        let order = test_order();
        let sig1 = sign_order(&wallet, &order, false).await.unwrap();
        let sig2 = sign_order(&wallet, &order, false).await.unwrap();
        assert_eq!(sig1, sig2, "same wallet + same order = same signature");
    }

    #[tokio::test]
    async fn test_sign_order_verifiable() {
        let wallet = test_wallet();
        let order = test_order();

        // Compute the digest ourselves
        let ds = compute_domain_separator(CTF_EXCHANGE).unwrap();
        let sh = hash_order(&order).unwrap();
        let digest = eip712_digest(&ds, &sh);

        // Sign and verify
        let sig_hex = sign_order(&wallet, &order, false).await.unwrap();
        let sig_bytes = hex::decode(&sig_hex[2..]).unwrap();
        let signature = Signature::try_from(sig_bytes.as_slice()).unwrap();

        // Recover the signer from the signature
        let recovered = signature.recover(H256::from(digest)).unwrap();
        assert_eq!(recovered, wallet.address(), "recovered address must match wallet");
    }

    #[tokio::test]
    async fn test_sign_order_neg_risk() {
        let wallet = test_wallet();
        let order = test_order();
        let sig_ctf = sign_order(&wallet, &order, false).await.unwrap();
        let sig_neg = sign_order(&wallet, &order, true).await.unwrap();
        // Different exchange address -> different domain separator -> different signature
        assert_ne!(sig_ctf, sig_neg);
    }

    #[test]
    fn test_hash_order_invalid_maker() {
        let mut order = test_order();
        order.maker = "not-an-address".to_string();
        assert!(hash_order(&order).is_err());
    }
}
