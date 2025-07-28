pub const SALT_SIZE: usize = 16;
pub const KEY_SIZE: usize = 32;
pub const PREIMAGE_SIZE: usize = 8;

pub fn gen_rand_preimage(n_bits: u8) -> anyhow::Result<[u8; PREIMAGE_SIZE]> {
    anyhow::ensure!((1..=63).contains(&n_bits), "n must be between 1 and 63");
    let mut rng = rand::rngs::ThreadRng::default();
    let preimage = core_gen_rand_preimage(n_bits, &mut rng);
    Ok(preimage)
}

#[inline]
pub fn core_gen_rand_preimage(n_bits: u8, rng: &mut rand::rngs::ThreadRng) -> [u8; PREIMAGE_SIZE] {
    assert!((1..=63).contains(&n_bits), "n must be between 1 and 63");
    let low = 1u64 << (n_bits - 1); // inclusive lower bound
    let high = 1u64 << n_bits; // exclusive upper bound
    let result = rand::Rng::random_range(rng, low..high); // uniform in [low, high)
    result.to_be_bytes()
}

#[cfg(feature = "alkali")]
fn libsodium_argon2id_derive_key(
    password: &[u8],
    salt: &[u8; SALT_SIZE],
    ops_limit: usize,
    mem_limit_kbytes: usize,
) -> anyhow::Result<[u8; KEY_SIZE]> {
    let mut key = [0u8; KEY_SIZE];
    let mem_limit_bytes = mem_limit_kbytes
        .checked_mul(1024)
        .ok_or_else(|| anyhow::anyhow!("Memory limit overflow: {mem_limit_kbytes} KB * 1024"))?;
    alkali::hash::pbkdf::argon2id::derive_key(
        password,
        salt,
        ops_limit,
        mem_limit_bytes,
        &mut key[..],
    )?;
    Ok(key)
}

#[cfg(feature = "rust-argon2")]
fn rust_argon2_derive_key(
    password: &[u8],
    salt: &[u8; SALT_SIZE],
    ops_limit: u32,
    mem_limit_kbytes: u32,
) -> anyhow::Result<[u8; KEY_SIZE]> {
    let mut key = [0u8; KEY_SIZE];
    let config = argon2::Config {
        mem_cost: mem_limit_kbytes,
        time_cost: ops_limit,
        variant: argon2::Variant::Argon2id,
        ..Default::default()
    };
    let raw = argon2::hash_raw(password, salt, &config)?;
    key.copy_from_slice(&raw);
    Ok(key)
}

pub fn wskdf_derive_key(
    preimage: &[u8; PREIMAGE_SIZE],
    salt: &[u8; SALT_SIZE],
    ops_limit: u32,
    mem_limit_kbytes: u32,
) -> anyhow::Result<[u8; KEY_SIZE]> {
    #[cfg(feature = "alkali")]
    return libsodium_argon2id_derive_key(
        preimage,
        salt,
        ops_limit.try_into()?,
        mem_limit_kbytes.try_into()?,
    );
    #[cfg(feature = "rust-argon2")]
    return rust_argon2_derive_key(preimage, salt, ops_limit, mem_limit_kbytes);
    #[cfg(not(any(feature = "alkali", feature = "rust-argon2")))]
    anyhow::bail!(
        "no argon2 implementation enabled, {preimage:?}, {salt:?}, {ops_limit} {mem_limit_kbytes}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_key_regression() -> anyhow::Result<()> {
        let salt: [u8; SALT_SIZE] = hex::decode("000102030405060708090a0b0c0d0e0f")?
            .try_into()
            .map_err(|_| anyhow::Error::msg("salt is invalid length"))?;
        let preimage = hex::decode("000000000000000d")?
            .try_into()
            .map_err(|_| anyhow::Error::msg("preimage is invalid length"))?;
        let key = wskdf_derive_key(&preimage, &salt, 42, 256 * 1024)?;
        assert_eq!(
            hex::decode("dc6b9dbde1d29c7e76549cd3cddbc7edee76966bbc0cf7afb13134ae4f43a043")?,
            key,
        );
        Ok(())
    }
}
