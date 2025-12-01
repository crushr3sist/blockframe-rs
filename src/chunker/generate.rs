use super::Chunker;

use reed_solomon_simd::ReedSolomonEncoder;
impl Chunker {
    pub fn get_chunks(&self, file_data: &[u8]) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
        let total_len = file_data.len();
        let chunk_size = (total_len + 5) / 6; // Round up to ensure we don't create more than 6 chunks

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

    pub fn generate_parity_segmented(
        &self,
        segment_data: &[u8],
    ) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
        // create Reed-Solomon encoder
        let data_shards = 1;
        let parity_shards = 3;
        let shard_bytes = segment_data.len();

        let mut encoder = ReedSolomonEncoder::new(data_shards, parity_shards, shard_bytes)?;

        // Add the data shard
        encoder.add_original_shard(segment_data)?;

        // Encode and get result
        let result = encoder.encode()?;

        // Extract parity shards
        let parity: Vec<Vec<u8>> = result.recovery_iter().map(|shard| shard.to_vec()).collect();

        println!(
            "Generated {} parity chunks from {} data chunks",
            parity_shards, data_shards
        );

        Ok(parity)
    }

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
        let parity_chunks: Vec<Vec<u8>> =
            result.recovery_iter().map(|shard| shard.to_vec()).collect();

        println!(
            "Generated {} parity chunks from {} data chunks",
            parity_shards, data_shards
        );

        Ok(parity_chunks)
    }
}
