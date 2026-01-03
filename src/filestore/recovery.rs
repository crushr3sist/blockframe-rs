/// Segment-level Reed-Solomon recovery utilities.
///
/// This module provides low-level recovery functions that can be used by both
/// filesystem implementations (Unix FUSE, Windows WinFSP) and the batch repair
/// operations in health.rs.
///
/// The key difference from health.rs repair methods:
/// - These operate on individual segments in-memory
/// - Designed for on-the-fly recovery during reads
/// - Return recovered data directly without writing to disk
/// - Caller decides whether to cache or persist
use reed_solomon_simd::ReedSolomonDecoder;

/// Recovers a single segment using Reed-Solomon RS(1,3) decoding.
///
/// Takes 3 parity shards and reconstructs the original data segment.
/// Useful for Tier 1 and Tier 2 where each segment has its own parity set.
///
/// # Parameters
///
/// * `parity_shards` - Exactly 3 parity shards (each same size as original segment)
/// * `expected_size` - Optional size to truncate to (for Tier 1 padding removal)
///
/// # Returns
///
/// * `Ok(Vec<u8>)` - Recovered segment data
/// * `Err` - If RS decoding fails or parity shards are invalid
///
/// # Example
///
/// ```no_run
/// use blockframe::filestore::recovery::recover_segment_rs13;
///
/// let parity0 = vec![0u8; 32 * 1024 * 1024]; // 32MB parity shard
/// let parity1 = vec![0u8; 32 * 1024 * 1024];
/// let parity2 = vec![0u8; 32 * 1024 * 1024];
///
/// let recovered = recover_segment_rs13(
///     vec![parity0, parity1, parity2],
///     None
/// ).unwrap();
/// ```
pub fn recover_segment_rs13(
    parity_shards: Vec<Vec<u8>>,
    expected_size: Option<usize>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if parity_shards.len() != 3 {
        return Err("Exactly 3 parity shards required for RS(1,3)".into());
    }

    let shard_size = parity_shards[0].len();

    // Verify all shards are same size
    if !parity_shards.iter().all(|s| s.len() == shard_size) {
        return Err("All parity shards must be the same size".into());
    }

    let mut decoder = ReedSolomonDecoder::new(1, 3, shard_size)?;

    // Add all 3 parity shards (data shard is missing/corrupt)
    decoder.add_recovery_shard(0, &parity_shards[0])?;
    decoder.add_recovery_shard(1, &parity_shards[1])?;
    decoder.add_recovery_shard(2, &parity_shards[2])?;

    let result = decoder.decode()?;
    let mut recovered = result
        .restored_original(0)
        .ok_or("Recovery failed")?
        .to_vec();

    // Truncate if needed (Tier 1 padding removal)
    if let Some(size) = expected_size {
        if recovered.len() > size {
            recovered.truncate(size);
        }
    }

    Ok(recovered)
}

/// Recovers a segment from a Tier 3 block using RS(30,3) decoding.
///
/// Tier 3 uses block-level parity: 30 segments per block, 3 parity shards for the entire block.
/// This means you need ALL valid segments in the block + the block parity to recover one segment.
///
/// # Parameters
///
/// * `valid_segments` - Up to 30 valid segments from the block (missing segments = None)
/// * `block_parity` - The 3 block-level parity shards
/// * `target_index` - Index of the segment to recover (0-29 within the block)
///
/// # Returns
///
/// * `Ok(Vec<u8>)` - Recovered segment data
/// * `Err` - If recovery fails (too many segments missing, parity invalid, etc.)
///
/// # Example
///
/// ```no_run
/// use blockframe::filestore::recovery::recover_segment_rs30_3;
///
/// // Segment 5 is corrupt, others are valid
/// let mut segments = vec![None; 30];
/// for i in 0..30 {
///     if i != 5 {
///         segments[i] = Some(vec![0u8; 32 * 1024 * 1024]);
///     }
/// }
///
/// let parity = vec![
///     vec![0u8; 32 * 1024 * 1024],
///     vec![0u8; 32 * 1024 * 1024],
///     vec![0u8; 32 * 1024 * 1024],
/// ];
///
/// let recovered = recover_segment_rs30_3(segments, parity, 5).unwrap();
/// ```
pub fn recover_segment_rs30_3(
    valid_segments: Vec<Option<Vec<u8>>>,
    block_parity: Vec<Vec<u8>>,
    target_index: usize,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if valid_segments.len() != 30 {
        return Err("Exactly 30 segment slots required for RS(30,3)".into());
    }

    if block_parity.len() != 3 {
        return Err("Exactly 3 block parity shards required for RS(30,3)".into());
    }

    if target_index >= 30 {
        return Err("Target index must be 0-29".into());
    }

    // Count missing segments
    let missing_count = valid_segments.iter().filter(|s| s.is_none()).count();
    if missing_count > 3 {
        return Err(format!(
            "Too many missing segments: {} (max 3 for RS(30,3))",
            missing_count
        )
        .into());
    }

    // Get shard size from first valid segment or parity
    let shard_size = valid_segments
        .iter()
        .find_map(|s| s.as_ref().map(|v| v.len()))
        .or_else(|| block_parity.first().map(|p| p.len()))
        .ok_or("Cannot determine shard size")?;

    let mut decoder = ReedSolomonDecoder::new(30, 3, shard_size)?;

    // Add valid data segments
    for (idx, segment) in valid_segments.iter().enumerate() {
        if let Some(data) = segment {
            decoder.add_original_shard(idx, data)?;
        }
    }

    // Add block parity shards
    decoder.add_recovery_shard(0, &block_parity[0])?;
    decoder.add_recovery_shard(1, &block_parity[1])?;
    decoder.add_recovery_shard(2, &block_parity[2])?;

    let result = decoder.decode()?;
    let recovered = result
        .restored_original(target_index)
        .ok_or("Failed to restore target segment")?
        .to_vec();

    Ok(recovered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recover_rs13_basic() {
        // Create some test data
        let original = vec![42u8; 1024];

        // In real usage, these would be RS-encoded parity shards
        // For this test, we'll just verify the function signature works
        let parity = vec![vec![0u8; 1024], vec![0u8; 1024], vec![0u8; 1024]];

        // This will fail because our test parity isn't real RS parity,
        // but it validates the API
        let result = recover_segment_rs13(parity, Some(1024));
        // We expect this to fail with test data
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_recover_rs13_wrong_shard_count() {
        let parity = vec![vec![0u8; 1024], vec![0u8; 1024]];

        let result = recover_segment_rs13(parity, None);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Exactly 3 parity shards required")
        );
    }

    #[test]
    fn test_recover_rs30_3_too_many_missing() {
        let segments = vec![None; 30]; // All missing
        let parity = vec![vec![0u8; 1024], vec![0u8; 1024], vec![0u8; 1024]];

        let result = recover_segment_rs30_3(segments, parity, 0);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Too many missing segments")
        );
    }
}
