//! Android file picker via JNI.
//!
//! On Android, HTML file inputs don't work properly in WebView due to security restrictions.
//! This module provides native file picker functionality via JNI.
//!
//! Architecture:
//! 1. Rust calls `pick_audio_file()` which launches an Intent via JNI
//! 2. The Intent opens Android's file picker
//! 3. When user selects a file, Android calls `onActivityResult` on the Activity
//! 4. We've registered a native callback that gets invoked
//! 5. The callback reads the file via ContentResolver and sends it back through a channel

#[cfg(target_os = "android")]
mod android {
    use crate::platform_support::android_utils::{get_activity, with_jni};
    use jni::objects::{JObject, JString, JValue};
    use jni::sys::jobject;
    use jni::JNIEnv;
    use tokio::sync::oneshot;
    use tracing::{error, info};

    static FILE_PICKER_SENDER: std::sync::Mutex<Option<oneshot::Sender<Option<FilePickerResult>>>> =
        std::sync::Mutex::new(None);

    pub const FILE_PICKER_REQUEST_CODE: i32 = 9999;

    #[derive(Debug, Clone)]
    pub struct FilePickerResult {
        pub name: String,
        pub data: Vec<u8>,
    }

    /// Opens the Android file picker for audio files.
    /// Returns the selected file's name and contents, or None if cancelled.
    pub async fn pick_audio_file() -> Option<FilePickerResult> {
        let (tx, rx) = oneshot::channel();

        {
            let mut sender = FILE_PICKER_SENDER.lock().unwrap();
            if sender.is_some() {
                error!("File picker already in progress");
                return None;
            }
            *sender = Some(tx);
        }

        let launch_result = with_jni(|env| {
            let activity = get_activity(env)?;
            launch_file_picker(env, &activity)
        });

        if let Err(e) = launch_result {
            error!("Failed to launch file picker: {:?}", e);
            let mut sender = FILE_PICKER_SENDER.lock().unwrap();
            *sender = None;
            return None;
        }

        info!("File picker launched, waiting for result...");

        match rx.await {
            Ok(result) => result,
            Err(_) => {
                error!("File picker channel closed unexpectedly");
                None
            }
        }
    }

    fn launch_file_picker(env: &mut JNIEnv<'_>, activity: &JObject<'_>) -> jni::errors::Result<()> {
        let intent_class = env.find_class("android/content/Intent")?;

        let action_get_content = env.get_static_field(
            &intent_class,
            "ACTION_GET_CONTENT",
            "Ljava/lang/String;",
        )?.l()?;

        let intent = env.new_object(
            &intent_class,
            "(Ljava/lang/String;)V",
            &[JValue::Object(&action_get_content)],
        )?;

        let mime_type = env.new_string("audio/*")?;
        env.call_method(
            &intent,
            "setType",
            "(Ljava/lang/String;)Landroid/content/Intent;",
            &[JValue::Object(&mime_type)],
        )?;

        let category_openable = env.get_static_field(
            &intent_class,
            "CATEGORY_OPENABLE",
            "Ljava/lang/String;",
        )?.l()?;
        env.call_method(
            &intent,
            "addCategory",
            "(Ljava/lang/String;)Landroid/content/Intent;",
            &[JValue::Object(&category_openable)],
        )?;

        env.call_method(
            activity,
            "startActivityForResult",
            "(Landroid/content/Intent;I)V",
            &[JValue::Object(&intent), JValue::Int(FILE_PICKER_REQUEST_CODE)],
        )?;

        info!("startActivityForResult called with request code {}", FILE_PICKER_REQUEST_CODE);
        Ok(())
    }

    /// Handle the activity result. Called from the native onActivityResult hook.
    /// This function is exported so it can be called from within the same native library.
    pub fn handle_activity_result(request_code: i32, result_code: i32, data_ptr: jobject) {
        if request_code != FILE_PICKER_REQUEST_CODE {
            return;
        }

        info!("handle_activity_result: request_code={}, result_code={}", request_code, result_code);

        let result = with_jni(|env| {
            // RESULT_OK = -1
            if result_code == -1 && !data_ptr.is_null() {
                let data = unsafe { JObject::from_raw(data_ptr) };
                let uri = env.call_method(&data, "getData", "()Landroid/net/Uri;", &[])?.l()?;

                if !uri.is_null() {
                    return read_file_from_uri(env, &uri).map(Some);
                }
            }
            Ok(None)
        });

        let file_result = match result {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to process activity result: {:?}", e);
                None
            }
        };

        let mut sender = FILE_PICKER_SENDER.lock().unwrap();
        if let Some(tx) = sender.take() {
            let _ = tx.send(file_result);
        }
    }

    /// JNI entry point called from MainActivity when onActivityResult fires.
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_dev_dioxus_main_MainActivity_handleNativeActivityResult(
        _env: JNIEnv<'_>,
        _class: JObject<'_>,
        request_code: jni::sys::jint,
        result_code: jni::sys::jint,
        data: JObject<'_>,
    ) {
        handle_activity_result(request_code, result_code, data.as_raw());
    }

    fn read_file_from_uri(env: &mut JNIEnv<'_>, uri: &JObject<'_>) -> jni::errors::Result<FilePickerResult> {
        let activity = get_activity(env)?;

        let content_resolver = env.call_method(
            &activity,
            "getContentResolver",
            "()Landroid/content/ContentResolver;",
            &[],
        )?.l()?;

        let name = get_display_name(env, &content_resolver, uri)?;
        info!("Reading file: {}", name);

        let input_stream = env.call_method(
            &content_resolver,
            "openInputStream",
            "(Landroid/net/Uri;)Ljava/io/InputStream;",
            &[JValue::Object(uri)],
        )?.l()?;

        let data = read_input_stream(env, &input_stream)?;
        info!("Read {} bytes", data.len());

        env.call_method(&input_stream, "close", "()V", &[])?;

        Ok(FilePickerResult { name, data })
    }

    fn get_display_name(
        env: &mut JNIEnv<'_>,
        content_resolver: &JObject<'_>,
        uri: &JObject<'_>,
    ) -> jni::errors::Result<String> {
        let projection = env.new_object_array(
            1,
            "java/lang/String",
            &env.new_string("_display_name")?,
        )?;

        let cursor = env.call_method(
            content_resolver,
            "query",
            "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;",
            &[
                JValue::Object(uri),
                JValue::Object(&projection),
                JValue::Object(&JObject::null()),
                JValue::Object(&JObject::null()),
                JValue::Object(&JObject::null()),
            ],
        )?.l()?;

        let mut name = String::from("unknown");

        if !cursor.is_null() {
            let has_first: bool = env.call_method(&cursor, "moveToFirst", "()Z", &[])?.z()?;
            if has_first {
                let display_name_str = env.new_string("_display_name")?;
                let col_index: i32 = env.call_method(
                    &cursor,
                    "getColumnIndex",
                    "(Ljava/lang/String;)I",
                    &[JValue::Object(&display_name_str)],
                )?.i()?;

                if col_index >= 0 {
                    let name_obj = env.call_method(
                        &cursor,
                        "getString",
                        "(I)Ljava/lang/String;",
                        &[JValue::Int(col_index)],
                    )?.l()?;

                    if !name_obj.is_null() {
                        let name_str: JString = name_obj.into();
                        name = env.get_string(&name_str)?.into();
                    }
                }
            }
            env.call_method(&cursor, "close", "()V", &[])?;
        }

        Ok(name)
    }

    fn read_input_stream(env: &mut JNIEnv<'_>, input_stream: &JObject<'_>) -> jni::errors::Result<Vec<u8>> {
        let mut data = Vec::new();
        let buffer = env.new_byte_array(8192)?;

        loop {
            let bytes_read: i32 = env.call_method(
                input_stream,
                "read",
                "([B)I",
                &[JValue::Object(&buffer)],
            )?.i()?;

            if bytes_read < 0 {
                break;
            }

            let mut chunk = vec![0i8; bytes_read as usize];
            env.get_byte_array_region(&buffer, 0, &mut chunk)?;
            data.extend(chunk.into_iter().map(|b| b as u8));
        }

        Ok(data)
    }
}

#[cfg(target_os = "android")]
pub use android::{pick_audio_file, FilePickerResult, FILE_PICKER_REQUEST_CODE};

#[cfg(not(target_os = "android"))]
#[derive(Debug, Clone)]
pub struct FilePickerResult {
    pub name: String,
    pub data: Vec<u8>,
}

#[cfg(not(target_os = "android"))]
pub async fn pick_audio_file() -> Option<FilePickerResult> {
    None
}
