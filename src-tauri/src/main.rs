// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("--speak-test") {
        let gender = match args.next().as_deref() {
            Some("male") => eql_alerts_lib::tts_voice_male(),
            _ => eql_alerts_lib::tts_voice_female(),
        };
        let text = args.next().unwrap_or_else(|| "Alert test".into());
        match eql_alerts_lib::tts_speak_now(&text, gender) {
            Ok(()) => println!("spoke ok"),
            Err(e) => {
                eprintln!("speak failed: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    eql_alerts_lib::run()
}
