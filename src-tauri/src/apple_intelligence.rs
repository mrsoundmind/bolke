use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

// Define the response structure from Swift
#[repr(C)]
pub struct AppleLLMResponse {
    pub response: *mut c_char,
    pub success: c_int,
    pub error_message: *mut c_char,
}

// Link to the Swift functions
extern "C" {
    pub fn is_apple_intelligence_available() -> c_int;
    pub fn free_apple_llm_response(response: *mut AppleLLMResponse);
}

// Safe wrapper functions
pub fn check_apple_intelligence_availability() -> bool {
    unsafe { is_apple_intelligence_available() == 1 }
}

// Link to the Swift functions for system prompt support
extern "C" {
    pub fn process_text_with_system_prompt_apple(
        system_prompt: *const c_char,
        user_content: *const c_char,
        max_tokens: i32,
    ) -> *mut AppleLLMResponse;
    pub fn process_text_plain_apple(
        system_prompt: *const c_char,
        user_content: *const c_char,
    ) -> *mut AppleLLMResponse;
}

/// Process text with Apple Intelligence using separate system prompt and user content
pub fn process_text_with_system_prompt(
    system_prompt: &str,
    user_content: &str,
    max_tokens: i32,
) -> Result<String, String> {
    let system_cstr = CString::new(system_prompt).map_err(|e| e.to_string())?;
    let user_cstr = CString::new(user_content).map_err(|e| e.to_string())?;

    let response_ptr = unsafe {
        process_text_with_system_prompt_apple(system_cstr.as_ptr(), user_cstr.as_ptr(), max_tokens)
    };
    take_response(response_ptr)
}

/// Like `process_text_with_system_prompt`, but the reply is plain text, not
/// a cleaned transcript: for replies with their own shape, like a summary.
pub fn process_text_plain(system_prompt: &str, user_content: &str) -> Result<String, String> {
    let system_cstr = CString::new(system_prompt).map_err(|e| e.to_string())?;
    let user_cstr = CString::new(user_content).map_err(|e| e.to_string())?;

    let response_ptr =
        unsafe { process_text_plain_apple(system_cstr.as_ptr(), user_cstr.as_ptr()) };
    take_response(response_ptr)
}

/// Reads the reply out of the Swift response, then frees it.
fn take_response(response_ptr: *mut AppleLLMResponse) -> Result<String, String> {
    if response_ptr.is_null() {
        return Err("Null response from Apple LLM".to_string());
    }

    let response = unsafe { &*response_ptr };

    let result = if response.success == 1 {
        if response.response.is_null() {
            Ok(String::new())
        } else {
            let c_str = unsafe { CStr::from_ptr(response.response) };
            let rust_str = c_str.to_string_lossy().into_owned();
            Ok(rust_str)
        }
    } else {
        let error_c_str = if !response.error_message.is_null() {
            unsafe { CStr::from_ptr(response.error_message) }
        } else {
            c"Unknown error"
        };
        let error_msg = error_c_str.to_string_lossy().into_owned();
        Err(error_msg)
    };

    // Clean up the response
    unsafe { free_apple_llm_response(response_ptr) };

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_availability() {
        let available = check_apple_intelligence_availability();
        println!("Apple Intelligence available: {}", available);
    }
}
