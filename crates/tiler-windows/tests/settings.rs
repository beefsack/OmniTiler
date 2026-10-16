use std::path::PathBuf;
use tiler_windows::settings::{
    BindingSetting, BindingState, LoadOutcome, PollOutcome, Preset, Settings, build_disabled,
    load_from_dir, parse_chord, poll_for_change, save_to_dir,
};
use tiler_windows::snapkey::{ChordDisable, ChordRemap, Classified, KeyboardConfig, SnapClassify};
use tiler_windows::tiling::{
    CliOverrides, TileOptions, apply_cli_overrides, parse_tile_args_from,
    parse_tile_args_from_with_overrides, tile_options_defaults,
};

static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct Temp {
    path: PathBuf,
}

impl Temp {
    fn new(name: &str) -> Self {
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "tiler-settings-{name}-{}-{t}-{n}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("temp dir");
        Self { path }
    }

    fn file(&self) -> PathBuf {
        self.path.join("settings.json")
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

fn takeover() -> KeyboardConfig {
    KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    }
}

const VK_LWIN: u32 = 91;
const VK_SHIFT: u32 = 16;
const VK_H: u32 = 0x48;
const VK_U: u32 = 0x55;
const VK_G: u32 = 0x47;
const VK_Q: u32 = 0x51;

#[path = "settings/catalog_families.rs"]
mod catalog_families;
#[path = "settings/preset_catalog.rs"]
mod preset_catalog;
#[path = "settings/remap_routing.rs"]
mod remap_routing;
#[path = "settings/schema_tokens.rs"]
mod schema_tokens;
#[path = "settings/store_file.rs"]
mod store_file;
