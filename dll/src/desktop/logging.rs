use core::sync::atomic::{AtomicBool, Ordering};

use log::LevelFilter;

use crate::desktop::dialogs::msg_box_ok;

pub static SHOULD_ENABLE_PANIC_HOOK: AtomicBool = AtomicBool::new(false);

#[cfg(all(feature = "use_fern_logger", not(feature = "use_pyo3_logger")))]
pub fn set_up_logging(log_level: LevelFilter) {
    use std::error::Error;

    use fern::InitError;

    /// Sets up the global logger
    fn set_up_logging_internal(log_level: LevelFilter) -> Result<(), InitError> {
        fern::Dispatch::new()
            .format(|out, message, record| {
                out.finish(format_args!(
                    "[{}][{}] {}",
                    record.level(),
                    record.target(),
                    message
                ))
            })
            .level(log_level)
            .chain(::std::io::stdout())
            .apply()?;
        Ok(())
    }

    match set_up_logging_internal(log_level) {
        Ok(_) => {}
        Err(e) => match e {
            InitError::Io(e) => {
                println!(
                    "[WARN] Logging IO init error: \r\nkind: \
                     {:?}\r\n\r\ndescription:\r\n{}\r\n\r\ncause:\r\n{:?}\r\n",
                    e.kind(),
                    e,
                    e.source()
                );
            }
            InitError::SetLoggerError(_) => {}
        },
    }
}

/// In the (rare) case of a panic, print it to the stdout, log it to the file and
/// prompt the user with a message box.
pub fn set_up_panic_hooks() {
    use std::panic::{self, PanicInfo};

    use backtrace::{Backtrace, BacktraceFrame};

    fn panic_fn(panic_info: &PanicInfo) {
        use std::thread;

        let payload = panic_info.payload();
        let location = panic_info.location();

        let payload_str = format!("{:?}", payload);
        let panic_str = payload
            .downcast_ref::<String>()
            .map(|s| s.as_ref())
            .or_else(|| payload.downcast_ref::<&str>().map(|s| *s))
            .unwrap_or(payload_str.as_str());

        let location_str = location.map(|loc| format!("{} at line {}", loc.file(), loc.line()));
        let backtrace_str_old = format_backtrace(&Backtrace::new());
        let backtrace_str = backtrace_str_old
            .lines()
            .filter(|l| !l.is_empty())
            .collect::<Vec<&str>>()
            .join("\r\n");
        // let backtrace_str = "";
        let thread = thread::current();
        let thread_name = thread.name().unwrap_or("<unnamed thread>");

        let error_str = format!(
            "An unexpected panic ocurred, the program has to exit.\r\nPlease report this error \
             and attach the log file found in the directory of the executable.\r\n\r\nThe error \
             ocurred in: {} in thread {}\r\n\r\nError \
             information:\r\n{}\r\n\r\nBacktrace:\r\n\r\n{}\r\n",
            location_str.unwrap_or(format!("<unknown location>")),
            thread_name,
            panic_str,
            backtrace_str
        );

        #[cfg(target_os = "linux")]
        let mut error_str_clone = error_str.clone();
        #[cfg(target_os = "linux")]
        {
            error_str_clone = error_str_clone.replace("<", "&lt;");
            error_str_clone = error_str_clone.replace(">", "&gt;");
        }

        // TODO: invoke external app crash handler with the location to the log file
        log::error!("{}", error_str);

        if SHOULD_ENABLE_PANIC_HOOK.load(Ordering::SeqCst) {
            #[cfg(not(target_os = "linux"))]
            tfd::MessageBox::new("Unexpected fatal error", &error_str)
                .with_icon(tfd::MessageBoxIcon::Info)
                .run_modal();

            #[cfg(target_os = "linux")]
            tfd::MessageBox::new("Unexpected fatal error", &error_str_clone)
                .with_icon(tfd::MessageBoxIcon::Info)
                .run_modal();
        }
    }

    fn format_backtrace(backtrace: &Backtrace) -> String {
        fn format_frame(frame: &BacktraceFrame) -> String {
            use std::ffi::OsStr;

            let ip = frame.ip();
            let symbols = frame.symbols();

            const UNRESOLVED_FN_STR: &str = "unresolved function";

            if symbols.is_empty() {
                return format!("{} @ {:?}", UNRESOLVED_FN_STR, ip);
            }

            // skip the first 10 symbols because they belong to the
            // backtrace library and aren't relevant for debugging
            symbols
                .iter()
                .map(|symbol| {
                    let mut nice_string = String::new();

                    if let Some(name) = symbol.name() {
                        let name_demangled = format!("{}", name);
                        let name_demangled_new = name_demangled
                            .rsplit("::")
                            .skip(1)
                            .map(|e| e.to_string())
                            .collect::<Vec<String>>();
                        let name_demangled = name_demangled_new
                            .into_iter()
                            .rev()
                            .collect::<Vec<String>>()
                            .join("::");
                        nice_string.push_str(&name_demangled);
                    } else {
                        nice_string.push_str(UNRESOLVED_FN_STR);
                    }

                    let mut file_string = String::new();
                    if let Some(file) = symbol.filename() {
                        let origin_file_name = file
                            .file_name()
                            .unwrap_or(OsStr::new("unresolved file name"))
                            .to_string_lossy();
                        file_string.push_str(&format!("{}", origin_file_name));
                    }

                    if let Some(line) = symbol.lineno() {
                        file_string.push_str(&format!(":{}", line));
                    }

                    if !file_string.is_empty() {
                        nice_string.push_str(" @ ");
                        nice_string.push_str(&file_string);
                        if !nice_string.ends_with("\n") {
                            nice_string.push_str("\n");
                        }
                    }

                    nice_string
                })
                .collect::<Vec<String>>()
                .join("")
        }

        backtrace
            .frames()
            .iter()
            .map(|frame| format_frame(frame))
            .collect::<Vec<String>>()
            .join("\r\n")
    }

    panic::set_hook(Box::new(panic_fn));
}
