use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    io,
    process::Command,
};

use cached::proc_macro::once;

#[cfg(target_family = "unix")]
use std::os::unix::ffi::OsStrExt;

#[cfg(target_family = "windows")]
use std::os::windows::ffi::OsStringExt;

#[derive(Debug, Clone)]
pub struct ConfigVariables {
    map: HashMap<String, String>,
}

impl ConfigVariables {
    pub fn get(&self, key: &str) -> Option<&String> {
        self.map.get(key)
    }
}

// frustratingly, something like the following does not exist in an
// OS-independent way in Rust
#[cfg(target_family = "unix")]
fn byte_array_to_os_string(bytes: &[u8]) -> OsString {
    let os_str = OsStr::from_bytes(bytes);
    os_str.to_os_string()
}

#[link(name = "kernel32")]
#[cfg(target_family = "windows")]
extern "system" {
    #[link_name = "GetConsoleCP"]
    fn get_console_code_page() -> u32;
    #[link_name = "MultiByteToWideChar"]
    fn multi_byte_to_wide_char(
        CodePage: u32,
        dwFlags: u32,
        lpMultiByteStr: *const u8,
        cbMultiByte: i32,
        lpWideCharStr: *mut u16,
        cchWideChar: i32,
    ) -> i32;
}

// convert bytes to wide-encoded characters on Windows
// from: https://stackoverflow.com/a/40456495/4975218
#[cfg(target_family = "windows")]
fn wide_from_console_string(bytes: &[u8]) -> Vec<u16> {
    assert!(bytes.len() < std::i32::MAX as usize);
    let mut wide;
    let mut len;
    unsafe {
        let cp = get_console_code_page();
        len = multi_byte_to_wide_char(
            cp,
            0,
            bytes.as_ptr() as *const u8,
            bytes.len() as i32,
            std::ptr::null_mut(),
            0,
        );
        wide = Vec::with_capacity(len as usize);
        len = multi_byte_to_wide_char(
            cp,
            0,
            bytes.as_ptr() as *const u8,
            bytes.len() as i32,
            wide.as_mut_ptr(),
            len,
        );
        wide.set_len(len as usize);
    }
    wide
}

#[cfg(target_family = "windows")]
fn byte_array_to_os_string(bytes: &[u8]) -> OsString {
    // first, use Windows API to convert to wide encoded
    let wide = wide_from_console_string(bytes);
    // then, use `std::os::windows::ffi::OsStringExt::from_wide()`
    OsString::from_wide(&wide)
}

// Execute R CMD config and return the captured output
fn r_cmd_config<S: AsRef<OsStr>>(r_binary: S) -> io::Result<OsString> {
    let out = Command::new(r_binary)
        .args(&["CMD", "config", "--all"])
        .output()?;

    // if there are any errors we print them out, helps with debugging
    if !out.stderr.is_empty() {
        println!(
            "> {}",
            byte_array_to_os_string(&out.stderr)
                .as_os_str()
                .to_string_lossy()
        );
    }

    Ok(byte_array_to_os_string(&out.stdout))
}

#[once]
pub fn build_r_cmd_configs() -> ConfigVariables {
    let r_configs = r_cmd_config("R");

    let mut rcmd_config_map = HashMap::new();
    match r_configs {
        Ok(configs) => {
            let input = configs.as_os_str().to_string_lossy();
            for line in input.lines() {
                // Ignore lines beyond comment marker
                if line.starts_with("##") {
                    break;
                }
                let parts: Vec<_> = line.split('=').map(str::trim).collect();
                if let [name, value] = parts.as_slice() {
                    rcmd_config_map.insert(name.to_string(), value.to_string());
                }
            }
        }
        _ => (),
    }
    // Return the struct
    ConfigVariables {
        map: rcmd_config_map,
    }
}

pub fn get_libs_and_paths(strings: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut paths: Vec<String> = Vec::new();
    let mut libs: Vec<String> = Vec::new();

    for s in &strings {
        let parts: Vec<&str> = s.split_whitespace().collect();
        for part in parts {
            if part.starts_with("-L") {
                paths.push(part[2..].to_string());
            } else if part.starts_with("-l") {
                libs.push(part[2..].to_string());
            }
        }
    }
    (paths, libs)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn get_cc() {
	let r_configs = build_r_cmd_configs();
	let value = r_configs.get("CC").expect("Unexpected missing value for R CMD config CC");
	assert!(!value.to_owned().is_empty(), "Value is empty");
    }

    #[test]
    fn get_fc() {
	let r_configs = build_r_cmd_configs();
	let value = r_configs.map.get("FC").expect("Unexpected missing value for R CMD config FC");
	assert!(!value.to_owned().is_empty(), "Value is empty");
    }
    
    #[test]
    fn get_blas_and_lapack() {
	let r_configs = build_r_cmd_configs();	
	let blas_libs = r_configs.get("BLAS_LIBS").expect("Unexpected missing value for R CMD config BLAS_LIBS").to_owned();
	let (_, lib) = get_libs_and_paths([ blas_libs ].to_vec());
	assert!(!lib.is_empty(), "Unexpected empty BLAS library");

	let lapack_libs = r_configs.get("LAPACK_LIBS").expect("Unexpected missing value for R CMD config LAPACK_LIBS").to_owned();
	let (_, lib) = get_libs_and_paths([ lapack_libs ].to_vec());
	assert!(!lib.is_empty(), "Unexpected empty LAPACK library");
    }
}

