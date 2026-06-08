pub const PIPE_NAME: &str = r"\\.\pipe\SteamVRVirtualHmdPipe";
pub const FRAME_SIZE: usize = std::mem::size_of::<HmdPoseData>();

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct HmdPoseData {
    pub position: [f32; 3],
    pub orientation: [f32; 4],
    pub connected: u32,
}

impl HmdPoseData {
    pub const fn standing_identity() -> Self {
        Self {
            position: [0.0, 1.75, -0.5],
            orientation: [0.0, 0.0, 0.0, 1.0],
            connected: 1,
        }
    }

    pub fn is_connected(self) -> bool {
        self.connected != 0
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self as *const HmdPoseData as *const u8,
                std::mem::size_of::<HmdPoseData>(),
            )
        }
    }

    pub fn from_bytes(bytes: &[u8; FRAME_SIZE]) -> Self {
        unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const HmdPoseData) }
    }
}

impl Default for HmdPoseData {
    fn default() -> Self {
        Self::standing_identity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_layout_is_c_compatible_and_stable() {
        assert_eq!(FRAME_SIZE, 32);
        assert_eq!(std::mem::align_of::<HmdPoseData>(), 4);
    }

    #[test]
    fn pose_round_trips_through_raw_bytes() {
        let pose = HmdPoseData {
            position: [1.0, 2.0, 3.0],
            orientation: [0.1, 0.2, 0.3, 0.9],
            connected: 0,
        };
        let mut bytes = [0_u8; FRAME_SIZE];
        bytes.copy_from_slice(pose.as_bytes());

        assert_eq!(HmdPoseData::from_bytes(&bytes), pose);
    }
}
