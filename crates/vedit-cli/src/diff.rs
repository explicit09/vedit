use anyhow::Result;
use std::path::Path;
use vedit_core::diff::diff;
use vedit_core::otio;

mod render;

pub fn run(before: &Path, after: &Path, json: bool) -> Result<()> {
    let before_tl = otio::load(before)?;
    let after_tl = otio::load(after)?;
    let changes = diff(&before_tl, &after_tl);

    if json {
        let s = serde_json::to_string_pretty(&changes)?;
        println!("{s}");
    } else {
        let lines = render::render(&changes);
        if lines.is_empty() {
            println!("No semantic changes detected.");
        } else {
            for line in lines {
                println!("{line}");
            }
        }
    }

    Ok(())
}
