use colored::Colorize;

pub fn status(message: impl AsRef<str>) {
    eprintln!("{} {}", "==>".blue().bold(), message.as_ref());
}

pub fn success(message: impl AsRef<str>) {
    eprintln!("{} {}", "ok".green().bold(), message.as_ref());
}

pub fn warn(message: impl AsRef<str>) {
    eprintln!("{} {}", "warn".yellow().bold(), message.as_ref());
}

pub fn error(message: impl AsRef<str>) {
    eprintln!("{} {}", "error".red().bold(), message.as_ref());
}

pub fn format_command(cmd: &std::process::Command) -> String {
    let program = cmd.get_program().to_string_lossy();
    let args = cmd
        .get_args()
        .map(|arg| arg.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    format!("{program} {args}")
}

pub fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;

    if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}
