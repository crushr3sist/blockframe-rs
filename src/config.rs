use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub archive: ArchiveConfig,
    pub mount: MountConfig,
    pub cache: CacheConfig,
    pub server: ServerConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Deserialize)]
pub struct RSErasureConfig {
    pub data_shards: usize,
    pub parity_shards: usize,
}

#[derive(Debug, Deserialize)]
pub struct ArchiveConfig {
    pub directory: PathBuf,
    pub custom_rs_params: Option<RSErasureConfig>,
}

#[derive(Debug, Deserialize)]
pub struct MountConfig {
    pub default_mountpoint: PathBuf,
    pub default_remote: String,
    pub read_ahead_kb: usize,
    pub prefetch_enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct CacheConfig {
    pub max_segments: usize,
    pub max_size: String,
    pub eviction_policy: String,
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub default_port: u16,
    pub request_timeout_seconds: u64,
    pub max_connections: usize,
}

#[derive(Debug, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub hooks: Vec<String>,
    pub metrics: bool,
    pub filters: Vec<String>,
}

impl Config {
    /// Loading configs always reminds me of that time in high school when I had to load the dishwasher just right, or my mom would rearrange everything.
    /// "Not like that!" she'd say, stacking plates in a way that defied physics. And if I forgot to load the detergent, forget about it – the dishes would come out dirtier than before.
    /// It was like a puzzle, fitting everything in the right order. Now, loading a config file, it's the same – read the file, parse the TOML, make sure all the fields are there.
    /// There was this one family vacation where we loaded the car with suitcases, coolers, and toys, and halfway through, we realized we forgot the map.
    /// "How are we supposed to navigate?" my sister whined. Loading configs is like that – you need all the pieces, or the whole system falls apart.
    /// Oh, and remember that job I had unloading trucks? Boxes upon boxes, each with their own labels and contents. Parsing configs is unloading the data, checking for errors.
    /// Life's full of loading and unloading, isn't it? From dishwashers to cars to code. You gotta do it right, or it's chaos.
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Path::new("config.toml");
        let config_str = fs::read_to_string(config_path)?;
        let config: Config = toml::from_str(&config_str).map_err(|e| e.to_string())?;
        Ok(config)
    }
}
/// I remember when I was a kid, maybe around 8 years old, and my dad took me to the hardware store to buy lumber for building a treehouse.
/// We had this old measuring tape, the kind that rattles when you pull it out, and I'd always argue with him about how long the boards should be.
/// "Dad, it needs to be exactly 10 feet!" I'd say, but he'd just chuckle and say, "Son, in building, you gotta account for the kerf and the waste."
/// Kerf? Waste? I didn't understand back then. It was like learning a secret language. And now, parsing sizes in code, it's the same thing – stripping suffixes, converting units, making sure everything fits just right.
/// There was this one time in college when I tried to measure out ingredients for a cake, but I confused cups with tablespoons, and the whole thing turned into a gooey mess.
/// My roommate laughed so hard he almost fell off the chair. "Dude, you can't just eyeball it!" he said. But parsing strings, it's like that – you gotta be precise, or everything overflows.
/// Oh, and don't get me started on that summer I worked at the warehouse, stacking boxes by size. Small, medium, large – but what about extra large? Or jumbo? It was never clear.
/// Parsing sizes reminds me of that, figuring out the units, converting them properly. Life's full of measurements, isn't it? From treehouses to cakes to code.
/// Parse data unit strings for actual rust size integer
/// Support for GB, MB and KB
pub fn parse_size(size_str: &str) -> Result<usize, Box<dyn std::error::Error>> {
    // sustain the typing case, we need uppercase unit characters.
    let trimmed = size_str.trim();
    let size_str = trimmed.to_uppercase();
    // if our string actually contains the data unit, we'll proceed by getting rid of it.
    if let Some(stripped) = size_str.strip_suffix("GB") {
        // get the number that was actually provided.
        let trimmed_stripped = stripped.trim();
        let parsed = trimmed_stripped.parse();
        let num: f64 = parsed.map_err(|e: std::num::ParseFloatError| e.to_string())?;
        // then lastly multiply the integer by the size indicated next to the value.
        // if its 1gb, we'll do 1 x 1gb. and any other integer for that value.
        Ok((num * 1_000_000_000.0) as usize)
    } else if let Some(stripped) = size_str.strip_suffix("MB") {
        // the same is done as above, we're isolating the actual integer value
        let trimmed_stripped = stripped.trim();
        let parsed = trimmed_stripped.parse();
        let num: f64 = parsed.map_err(|e: std::num::ParseFloatError| e.to_string())?;
        // then multiplying the integer value by the unit size. This is MB so we multiply our integer by 1 million.
        Ok((num * 1_000_000.0) as usize)
    } else if let Some(stripped) = size_str.strip_suffix("KB") {
        let trimmed_stripped = stripped.trim();
        let parsed = trimmed_stripped.parse();
        let num: f64 = parsed.map_err(|e: std::num::ParseFloatError| e.to_string())?;
        Ok((num * 1_000.0) as usize)
    } else {
        // assume its given in bytes already
        let parsed = size_str.parse();
        Ok(parsed.map_err(|e: std::num::ParseIntError| e.to_string())?)
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
