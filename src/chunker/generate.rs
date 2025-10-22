use super::Chunker;

use reed_solomon_erasure::galois_8::ReedSolomon;
impl Chunker {
    pub fn get_chunks(&self, file_data: &[u8]) -> Vec<Vec<u8>> {
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

        chunks
    }

    pub fn generate_parity(
        &self,
        data_chunks: &[Vec<u8>],
        data_shards: usize,
        parity_shards: usize,
    ) -> Result<Vec<Vec<u8>>, String> {
        // create Reed-Solomon encoded
        let encoder = ReedSolomon::new(data_shards, parity_shards)
            .map_err(|e| format!("Failed to create RS encoder: {:?}", e))?;

        // Find max chunk size (all chunks must be the same size for RS)
        let max_chunk_size = data_chunks
            .iter()
            .map(|chunk| chunk.len())
            .max()
            .ok_or("No chunks provided")?;

        // Pad all data chunks to max size
        let mut padded_chunks: Vec<Vec<u8>> = data_chunks
            .iter()
            .map(|chunk| {
                let mut padded = chunk.clone();
                padded.resize(max_chunk_size, 0);
                padded
            })
            .collect();

        // create empty parity chunks
        let mut parity_chunks: Vec<Vec<u8>> = vec![vec![0u8; max_chunk_size]; parity_shards];
        // combine data + parity for encoding
        let mut all_shards: Vec<&mut [u8]> = padded_chunks
            .iter_mut()
            .map(|v| v.as_mut_slice())
            .chain(parity_chunks.iter_mut().map(|v| v.as_mut_slice()))
            .collect();

        // magic: generate parity data
        encoder
            .encode(&mut all_shards)
            .map_err(|e| format!("RS encoding failed: {:?}", e))?;

        println!(
            "Generated {} parity chunks from {} data chunks",
            parity_shards, data_shards
        );

        Ok(parity_chunks)
    }
}
