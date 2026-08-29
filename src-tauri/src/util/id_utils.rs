//! UUID generation (ported from electron/utils/IdUtils.ts).

use uuid::Uuid;

pub struct IdUtils;

impl IdUtils {
    pub fn gen_uuid() -> String {
        Uuid::new_v4().to_string()
    }
}
