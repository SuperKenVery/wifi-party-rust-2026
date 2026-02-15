//! Android JNI utilities.
//!
//! Common utilities for interacting with Android via JNI, shared across
//! multicast_lock, file_picker, and other Android-specific code.

#[cfg(target_os = "android")]
mod android {
    use jni::objects::JObject;
    use jni::sys::jobject;
    use jni::JNIEnv;
    use ndk_context::AndroidContext;
    use std::sync::Arc;

    /// Get the Android context from NDK.
    pub fn get_context() -> AndroidContext {
        ndk_context::android_context()
    }

    /// Run a closure with an attached JNI environment.
    /// Use this when you only need the JNIEnv without the Activity context.
    pub fn with_jni<F, R>(closure: F) -> jni::errors::Result<R>
    where
        for<'j> F: FnOnce(&mut JNIEnv<'j>) -> jni::errors::Result<R>,
    {
        let context = get_context();
        let vm = Arc::new(unsafe { jni::JavaVM::from_raw(context.vm().cast())? });
        jni::Executor::new(vm).with_attached(|env| closure(env))
    }

    /// Run a closure with an attached JNI environment and the Activity context.
    /// Use this when you need to call methods on the Activity.
    pub fn with_jni_context<F, R>(closure: F) -> jni::errors::Result<R>
    where
        for<'j> F: FnOnce(&mut JNIEnv<'j>, JObject<'j>) -> jni::errors::Result<R>,
    {
        let context = get_context();
        let vm = Arc::new(unsafe { jni::JavaVM::from_raw(context.vm().cast())? });
        let ctx = context.context();
        let ctx = unsafe { JObject::from_raw(ctx as jobject) };
        jni::Executor::new(vm).with_attached(|env| closure(env, ctx))
    }

    /// Get the Activity as a JObject from JNIEnv.
    /// Useful when you're already in a JNI context and need the Activity.
    pub fn get_activity<'j>(_env: &mut JNIEnv<'j>) -> jni::errors::Result<JObject<'j>> {
        let context = get_context();
        let ctx = unsafe { JObject::from_raw(context.context() as jobject) };
        Ok(ctx)
    }
}

#[cfg(target_os = "android")]
pub use android::{get_activity, get_context, with_jni, with_jni_context};
