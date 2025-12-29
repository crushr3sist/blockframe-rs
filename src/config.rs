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
/// Parse data unit strings for actual rust size integer
/// Support for GB, MB and KB
pub fn parse_size(size_str: &str) -> Result<usize, Box<dyn std::error::Error>> {
    // sustain the typing case, we need uppercase unit characters.
    let size_str = size_str.trim().to_uppercase();
    // if our string actually contains the data unit, we'll proceed by getting rid of it.
    if let Some(stripped) = size_str.strip_suffix("GB") {
        // get the number that was actually provided.
        let num: f64 = stripped
            .trim()
            .parse()
            .map_err(|e: std::num::ParseFloatError| e.to_string())?;
        // then lastly multiply the integer by the size indicated next to the value.
        // if its 1gb, we'll do 1 x 1gb. and any other integer for that value.
        Ok((num * 1_000_000_000.0) as usize)
    } else if let Some(stripped) = size_str.strip_suffix("MB") {
        // the same is done as above, we're isolating the actual integer value
        let num: f64 = stripped
            .trim()
            .parse()
            .map_err(|e: std::num::ParseFloatError| e.to_string())?;
        // then multiplying the integer value by the unit size. This is MB so we multiply our integer by 1 million.
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

    /// very simple assert test
    /// checking to see if our strings match the actual byte values that rust needs
    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("1GB").unwrap(), 1_000_000_000);
        assert_eq!(parse_size("500MB").unwrap(), 500_000_000);
        assert_eq!(parse_size("1024").unwrap(), 1024);
    }
}
