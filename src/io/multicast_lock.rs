//! Android MulticastLock management via JNI.
//!
//! On Android, multicast UDP packets are filtered by default to save battery.
//! This module acquires a `WifiManager.MulticastLock` to allow receiving multicast traffic.

#[cfg(target_os = "android")]
mod android {
    use crate::platform_support::android_utils::with_jni_context;
    use jni::objects::JObject;
    use jni::JNIEnv;
    use tracing::{error, info};

    pub struct MulticastLock {
        lock: jni::objects::GlobalRef,
    }

    impl MulticastLock {
        pub fn acquire() -> Option<Self> {
            match with_jni_context(|env, ctx| acquire_lock(env, &ctx)) {
                Ok(lock) => {
                    info!("MulticastLock acquired");
                    Some(Self { lock })
                }
                Err(e) => {
                    error!("Failed to acquire MulticastLock: {:?}", e);
                    None
                }
            }
        }
    }

    impl Drop for MulticastLock {
        fn drop(&mut self) {
            if let Err(e) = with_jni_context(|env, _ctx| release_lock(env, &self.lock)) {
                error!("Failed to release MulticastLock: {:?}", e);
            } else {
                info!("MulticastLock released");
            }
        }
    }

    fn acquire_lock<'j>(
        env: &mut JNIEnv<'j>,
        context: &JObject<'j>,
    ) -> jni::errors::Result<jni::objects::GlobalRef> {
        let wifi_manager = env
            .call_method(
                context,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[(&env.new_string("wifi")?).into()],
            )?
            .l()?;

        let lock_name = env.new_string("wifi-party-multicast")?;
        let multicast_lock = env
            .call_method(
                &wifi_manager,
                "createMulticastLock",
                "(Ljava/lang/String;)Landroid/net/wifi/WifiManager$MulticastLock;",
                &[(&lock_name).into()],
            )?
            .l()?;

        env.call_method(
            &multicast_lock,
            "setReferenceCounted",
            "(Z)V",
            &[false.into()],
        )?;

        env.call_method(&multicast_lock, "acquire", "()V", &[])?;

        env.new_global_ref(&multicast_lock)
    }

    fn release_lock(
        env: &mut JNIEnv<'_>,
        lock: &jni::objects::GlobalRef,
    ) -> jni::errors::Result<()> {
        let is_held: bool = env.call_method(lock.as_obj(), "isHeld", "()Z", &[])?.z()?;

        if is_held {
            env.call_method(lock.as_obj(), "release", "()V", &[])?;
        }

        Ok(())
    }
}

#[cfg(target_os = "android")]
pub use android::MulticastLock;

#[cfg(not(target_os = "android"))]
pub struct MulticastLock;

#[cfg(not(target_os = "android"))]
impl MulticastLock {
    pub fn acquire() -> Option<Self> {
        Some(Self)
    }
}
