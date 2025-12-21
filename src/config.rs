use serde::Deserialize;
use std::{fs, path::Path};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub cache: CacheConfig,
}

#[derive(Debug, Deserialize)]
pub struct CacheConfig {
    pub max_segments: usize,
    pub max_size: String,
}

impl Config {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_str = fs::read_to_string(Path::new(".cargo/config.toml"))?;
        let config: Config = toml::from_str(&config_str).map_err(|e| e.to_string())?;
        Ok(config)
    }
}

pub fn parse_size(size_str: &str) -> Result<usize, Box<dyn std::error::Error>> {
    let size_str = size_str.trim().to_uppercase();
    if let Some(stripped) = size_str.strip_suffix("GB") {
        let num: f64 = stripped
            .trim()
            .parse()
            .map_err(|e: std::num::ParseFloatError| e.to_string())?;
        Ok((num * 1_000_000_000.0) as usize)
    } else if let Some(stripped) = size_str.strip_suffix("MB") {
        let num: f64 = stripped
            .trim()
            .parse()
            .map_err(|e: std::num::ParseFloatError| e.to_string())?;
        Ok((num * 1_000_000.0) as usize)
    } else if let Some(stripped) = size_str.strip_suffix("KB") {
        let num: f64 = stripped
            .trim()
            .parse()
            .map_err(|e: std::num::ParseFloatError| e.to_string())?;
        Ok((num * 1_000.0) as usize)
    } else {
        // assume its given in bytes already
        Ok(size_str
            .parse()
            .map_err(|e: std::num::ParseIntError| e.to_string())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("1GB").unwrap(), 1_000_000_000);
        assert_eq!(parse_size("500MB").unwrap(), 500_000_000);
        assert_eq!(parse_size("1024").unwrap(), 1024);
    }
}
