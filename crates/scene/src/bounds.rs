use crate::Region;
use crate::error::{SceneError, SceneErrorCode};

/// Axis-aligned scene bounds with an exclusive maximum corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub min: [i64; 3],
    pub max_exclusive: [i64; 3],
}

impl Bounds {
    pub fn from_regions(regions: &[Region]) -> Result<Self, SceneError> {
        if regions.is_empty() {
            return Err(SceneError::new(
                SceneErrorCode::EmptyScene,
                "scene has no regions",
            ));
        }

        let mut min = [i64::MAX; 3];
        let mut max = [i64::MIN; 3];

        for region in regions {
            for axis in 0..3 {
                let start = region.origin[axis];
                let end = start
                    .checked_add(i64::from(region.size[axis]))
                    .ok_or_else(|| {
                        SceneError::new(SceneErrorCode::BoundsOverflow, "region bound overflow")
                    })?;
                min[axis] = min[axis].min(start);
                max[axis] = max[axis].max(end);
            }
        }

        Ok(Self {
            min,
            max_exclusive: max,
        })
    }

    pub fn size(&self) -> [u64; 3] {
        let mut result = [0u64; 3];
        for (axis, value) in result.iter_mut().enumerate() {
            *value = u64::try_from(self.max_exclusive[axis] - self.min[axis]).unwrap_or(0);
        }
        result
    }

    pub fn volume(&self) -> Option<u64> {
        let size = self.size();
        size[0]
            .checked_mul(size[1])
            .and_then(|value| value.checked_mul(size[2]))
    }
}
