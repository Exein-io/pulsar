use std::{fmt, str::FromStr};

use bpf_common::{
    aya::{self, maps::MapError},
    Pid,
};

use super::config::{Rule, MAX_IMAGE_LEN};

/// This map assigns to every running process a PolicyDecision:
/// - Are we interested in events generated by this process?
/// - Are we interested in events generated by its children?
pub(crate) struct InterestMap(pub(crate) aya::maps::HashMap<aya::maps::MapRefMut, i32, u8>);

impl InterestMap {
    /// Try to load the map from eBPF
    pub(crate) fn load(bpf: &mut aya::Bpf) -> Result<Self, MapError> {
        let map = aya::maps::HashMap::try_from(bpf.map_mut("map_interest")?)?;
        Ok(Self(map))
    }

    /// Clear the map
    pub(crate) fn clear(&mut self) -> Result<(), MapError> {
        let old_processes: Result<Vec<i32>, _> = self.0.keys().collect();
        old_processes?
            .iter()
            .try_for_each(|pid| self.0.remove(pid))?;
        Ok(())
    }

    /// Update the interest map by setting the policy decision of a given process
    pub(crate) fn set(&mut self, pid: Pid, policy_result: PolicyDecision) -> Result<(), MapError> {
        self.0.insert(pid.as_raw(), policy_result.as_raw(), 0)?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(crate) struct PolicyDecision {
    pub(crate) interesting: bool,
    pub(crate) children_interesting: bool,
}

impl Default for PolicyDecision {
    fn default() -> Self {
        Self {
            interesting: true,
            children_interesting: true,
        }
    }
}

impl PolicyDecision {
    /// Convert the `PolicyDecision` to a bit field
    pub(crate) fn as_raw(&self) -> u8 {
        match (self.children_interesting, self.interesting) {
            (false, false) => 0,
            (false, true) => 1,
            (true, false) => 2,
            (true, true) => 3,
        }
    }
}

/// A RuleMap contains the target/whitelist images and weather or not the rule
/// should affect its children.
/// Whitelist and target list have the same fields, so we use a single struct for both.
pub(crate) struct RuleMap(aya::maps::HashMap<aya::maps::MapRefMut, Image, u8>);

impl RuleMap {
    /// Try to load the whitelist map
    pub(crate) fn whitelist(bpf: &mut aya::Bpf) -> Result<Self, MapError> {
        let map = aya::maps::HashMap::try_from(bpf.map_mut("whitelist")?)?;
        Ok(Self(map))
    }

    /// Try to load the target map
    pub(crate) fn target(bpf: &mut aya::Bpf) -> Result<Self, MapError> {
        let map = aya::maps::HashMap::try_from(bpf.map_mut("target")?)?;
        Ok(Self(map))
    }

    /// Clear the map
    pub(crate) fn clear(&mut self) -> Result<(), MapError> {
        let old_processes: Result<Vec<_>, _> = self.0.keys().collect();
        old_processes?
            .iter()
            .try_for_each(|image| self.0.remove(image))?;
        Ok(())
    }

    /// Fill the map with a list of rules
    pub(crate) fn install(&mut self, rules: &Vec<Rule>) -> Result<(), MapError> {
        for rule in rules {
            let value: u8 = if rule.with_children { 1 } else { 0 };
            self.0.insert(rule.image, value, 0)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Image(pub(crate) [u8; MAX_IMAGE_LEN]);
// We must explicitly mark Image as a plain old data which can be safely memcopied by aya.
unsafe impl bpf_common::aya::Pod for Image {}

impl FromStr for Image {
    type Err = String;

    fn from_str(image: &str) -> Result<Self, Self::Err> {
        if !image.is_ascii() {
            Err("process image must be ascii".to_string())
        } else if image.len() >= MAX_IMAGE_LEN {
            Err(format!(
                "process image must be smaller than {MAX_IMAGE_LEN}"
            ))
        } else {
            let mut image_vec: Vec<u8> = image.bytes().collect();
            image_vec.resize(MAX_IMAGE_LEN, 0);
            let mut image_array = [0; MAX_IMAGE_LEN];
            image_array.clone_from_slice(&image_vec[..]);
            Ok(Image(image_array))
        }
    }
}

impl fmt::Display for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for c in self.0.into_iter().take_while(|c| *c != 0) {
            write!(f, "{}", c as char)?;
        }
        Ok(())
    }
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Image").field(&self.to_string()).finish()
    }
}
