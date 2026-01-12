use super::Chunker;

use reed_solomon_simd::ReedSolomonEncoder;
use tracing::debug;
impl Chunker {
    /// Get chunks, like dividing a cake into equal slices for sharing. "Fair portions," mom would say.
    /// I'd measure carefully, cut evenly. "Everyone gets some!"
    /// Getting chunks is like that – divide data into 6 parts. "Distributed!"
    /// There was this cake that was uneven, learned to measure properly. Precision matters.
    /// Life's about division, from cakes to data.
    pub fn get_chunks(&self, file_data: &[u8]) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
        let total_len = file_data.len();
        let chunk_size = total_len.div_ceil(6); // Round up to ensure we don't create more than 6 chunks

        let mut chunks = Vec::new();

        for i in 0..6 {
            let start = i * chunk_size;
            let end = ((i + 1) * chunk_size).min(total_len);

            if start < total_len {
                chunks.push(file_data[start..end].to_vec());
            } else {
                // If we've exhausted the data, push empty chunks
                chunks.push(vec![]);
            }
        }

        Ok(chunks)
    }

    /// Generate parity segmented, like adding extra locks to a safe. "Triple protection," the bank says.
    /// I'd encode with Reed-Solomon, create parity shards. "Secure!"
    /// Generating parity segmented is like that – RS encoder, pad to 64, encode. "Redundancy added!"
    /// There was this safe that needed better locks, upgraded it. Security first.
    /// Life's about protection, from safes to data.
    pub fn generate_parity_segmented(
        &self,
        segment_data: &[u8],
    ) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
        // create Reed-Solomon encoder
        let data_shards = 1;
        let parity_shards = 3;
        // calculate the padded size, round up to the nearest 64
        let padded_size = segment_data.len().div_ceil(64) * 64;

        // initalise the encoder with the padded size
        let mut encoder = ReedSolomonEncoder::new(data_shards, parity_shards, padded_size)?;

        if segment_data.len() < padded_size {
            // create a temporary padded vector if strict alignment is needed
            let mut padded_vec = segment_data.to_vec();
            padded_vec.resize(padded_size, 0);
            encoder.add_original_shard(&padded_vec)?;
        } else {
            // the faster path as most segments will be aligned already
            encoder.add_original_shard(segment_data)?;
        }
        let result = encoder.encode()?;
        let recovery_iter = result.recovery_iter();
        let mapped = recovery_iter.map(|shard| shard.to_vec());
        let parity: Vec<Vec<u8>> = mapped.collect();

        debug!(
            "Generated {} parity chunks from {} data chunks",
            parity_shards, data_shards
        );

        Ok(parity)
    }

    /// Generate parity, like creating backup copies of important documents. "Never lose this," I'd think.
    /// I'd encode segments with RS, create parity shards. "Protected!"
    /// Generating parity is like that – find max size, pad, encode. "Safety net!"
    /// There was this document I almost lost, started backing up everything. Relief.
    /// Life's about backups, from documents to segments.
    pub fn generate_parity(
        &self,
        segments: &[&[u8]],
        data_shards: usize,
        parity_shards: usize,
    ) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
        // Find max chunk size (all chunks must be the same size for RS)
        let max_chunk_size = segments
            .iter()
            .map(|chunk| chunk.len())
            .max()
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "No chunks provided")
            })?;

        // Pad all data chunks to max size
        let padded_chunks: Vec<Vec<u8>> = segments
            .iter()
            .map(|chunk| {
                let mut padded = chunk.to_vec();
                padded.resize(max_chunk_size, 0);
                padded
            })
            .collect();

        let mut encoder = ReedSolomonEncoder::new(data_shards, parity_shards, max_chunk_size)?;

        // Add all data shards
        for shard in padded_chunks.iter() {
            encoder.add_original_shard(shard)?;
        }

        // Encode and get result
        let result = encoder.encode()?;

        // Extract parity shards
        let recovery_iter = result.recovery_iter();
        let mapped = recovery_iter.map(|shard| shard.to_vec());
        let parity_chunks: Vec<Vec<u8>> = mapped.collect();

        debug!(
            "Generated {} parity chunks from {} data chunks",
            parity_shards, data_shards
        );

        Ok(parity_chunks)
    }
}
