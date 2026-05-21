//! vedit's internal timeline model.
//!
//! This is deliberately a subset of OTIO. We only model what the diff and
//! merge engines need: tracks, clips, transitions, source ranges, and media
//! references. Anything else from the source OTIO is preserved as opaque
//! JSON on the parent object so we can write it back unchanged later.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    pub name: String,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub name: String,
    pub kind: TrackKind,
    /// Children appear in timeline order. Clips and transitions interleave
    /// the way OTIO emits them: a transition sits between two adjacent clips.
    pub children: Vec<TrackChild>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Video,
    Audio,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrackChild {
    Clip(Clip),
    Transition(Transition),
    Gap(Gap),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip_id: Option<String>,
    pub name: String,
    pub media_reference: Option<String>,
    pub source_range: Option<TimeRange>,
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect_name: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub name: String,
    pub duration: Option<RationalTime>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gap {
    pub duration: Option<RationalTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RationalTime {
    pub value: f64,
    pub rate: f64,
}

impl RationalTime {
    pub fn frames(&self) -> f64 {
        self.value
    }
    pub fn seconds(&self) -> f64 {
        if self.rate > 0.0 {
            self.value / self.rate
        } else {
            self.value
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TimeRange {
    pub start_time: RationalTime,
    pub duration: RationalTime,
}

impl TimeRange {
    pub fn end_time(&self) -> RationalTime {
        RationalTime {
            value: self.start_time.value + self.duration.value,
            rate: self.start_time.rate,
        }
    }
}
