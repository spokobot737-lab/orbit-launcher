use crate::{db, post_launch, AppState};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::path::Path;
pub fn validate(v: &Value) -> Result<()> {
    let p = Path::new(v["path"].as_str().unwrap_or(""));
    ensure!(
        p.is_absolute() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")),
        "Choose an absolute path to a Windows executable (.exe)"
    );
    ensure!(
        v["args"].as_array().is_some_and(
            |a| a.len() < 100 && a.iter().all(|x| x.as_str().is_some_and(|s| s.len() < 8192))
        ),
        "Arguments must be a JSON array of strings"
    );
    let wd = v["workingDir"].as_str().unwrap_or("");
    ensure!(
        wd.is_empty() || Path::new(wd).is_absolute(),
        "Working directory must be an absolute path"
    );
    Ok(())
}
pub async fn import(_s: &AppState, v: &Value) -> Result<Value> {
    let input = v["path"].as_str().unwrap_or("");
    let p = Path::new(input);
    ensure!(
        p.is_absolute() && p.is_file(),
        "This file is missing or has moved."
    );
    let ext = p
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    ensure!(
        ["exe", "lnk"].contains(&ext.as_str()),
        "Choose an EXE or Windows shortcut"
    );
    let script = r#"$ErrorActionPreference='Stop'; [Console]::OutputEncoding=New-Object Text.UTF8Encoding; $p=$env:ORBIT_IMPORT; $rawArgs=''; $wd=''; if([IO.Path]::GetExtension($p) -ieq '.lnk'){ $sc=(New-Object -ComObject WScript.Shell).CreateShortcut($p); $p=$sc.TargetPath; $wd=$sc.WorkingDirectory; $rawArgs=$sc.Arguments }; $icon=''; try { Add-Type -AssemblyName System.Drawing; $i=[Drawing.Icon]::ExtractAssociatedIcon($p); $b=$i.ToBitmap(); $ms=New-Object IO.MemoryStream; $b.Save($ms,[Drawing.Imaging.ImageFormat]::Png); $icon='data:image/png;base64,'+[Convert]::ToBase64String($ms.ToArray()); $b.Dispose(); $i.Dispose(); $ms.Dispose() } catch {}; @{path=$p;name=[IO.Path]::GetFileNameWithoutExtension($env:ORBIT_IMPORT);rawArgs=$rawArgs;args=@();workingDir=$wd;image=$icon} | ConvertTo-Json -Compress"#;
    let mut cmd = tokio::process::Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("ORBIT_IMPORT", input);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let output = cmd.output().await?;
    ensure!(
        output.status.success(),
        "Could not resolve shortcut. Add its executable directly."
    );
    let mut value: Value = serde_json::from_slice(&output.stdout)?;
    value["args"] = json!(parse_arguments(value["rawArgs"].as_str().unwrap_or(""))?);
    value.as_object_mut().unwrap().remove("rawArgs");
    value["id"] = json!(uuid::Uuid::new_v4().to_string());
    value["kind"] = json!("program");
    value["favorite"] = json!(false);
    validate(&value)?;
    Ok(value)
}
fn parse_arguments(raw: &str) -> Result<Vec<String>> {
    if raw.is_empty() {
        return Ok(vec![]);
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{Foundation::LocalFree, UI::Shell::CommandLineToArgvW};
        let wide: Vec<u16> = format!("orbit.exe {raw}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut count = 0;
        let ptr = CommandLineToArgvW(wide.as_ptr(), &mut count);
        ensure!(!ptr.is_null(), "Invalid shortcut arguments");
        let mut args = vec![];
        for i in 1..count {
            let p = *ptr.add(i as usize);
            let mut len = 0;
            while *p.add(len) != 0 {
                len += 1;
            }
            args.push(String::from_utf16_lossy(std::slice::from_raw_parts(p, len)));
        }
        LocalFree(ptr.cast());
        Ok(args)
    }
    #[cfg(not(windows))]
    {
        Ok(shell_words::split(raw)?)
    }
}
pub async fn image(v: &Value) -> Result<Value> {
    use base64::Engine;
    let path = Path::new(v["path"].as_str().unwrap_or(""));
    ensure!(path.is_absolute() && path.is_file(), "Choose an image file");
    ensure!(
        tokio::fs::metadata(path).await?.len() <= 5_000_000,
        "Choose an image under 5 MB"
    );
    let bytes = tokio::fs::read(path).await?;
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else {
        anyhow::bail!("Choose a PNG or JPEG image")
    };
    Ok(json!(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_arguments() {
        assert_eq!(
            parse_arguments(r#"--file "C:\My Files\world.txt" --flag"#).unwrap(),
            vec!["--file", r#"C:\My Files\world.txt"#, "--flag"]
        );
    }
}
pub async fn launch(app: Option<&tauri::AppHandle>, s: &AppState, v: &Value) -> Result<Value> {
    let id = v["id"].as_str().unwrap_or("");
    let item = db::item(s, id)?;
    ensure!(item["kind"] == "program", "Not a program");
    validate(&item)?;
    let path = Path::new(item["path"].as_str().unwrap());
    ensure!(
        path.is_file(),
        "The executable is missing or has moved. Edit this entry to locate it."
    );
    let mut cmd = tokio::process::Command::new(path);
    for arg in item["args"].as_array().unwrap() {
        cmd.arg(arg.as_str().unwrap());
    }
    let wd = item["workingDir"]
        .as_str()
        .filter(|x| !x.is_empty())
        .map(Path::new)
        .unwrap_or(path.parent().unwrap());
    ensure!(wd.is_dir(), "Working directory is missing");
    cmd.current_dir(wd);
    let mut child = cmd.spawn()?;
    let pid = child.id();
    if let Some(app) = app {
        use tauri::Emitter;
        let _ = app.emit("process", json!({"id":id,"running":true,"pid":pid}));
    }
    db::record(s, id)?;
    if item["gameActivity"] == true {
        crate::activity::game_started(s, &item, pid);
    }
    let handle = app.cloned();
    let item_id = id.to_owned();
    tauri::async_runtime::spawn(async move {
        let result = child.wait().await;
        use tauri::Emitter;
        if let Some(handle) = handle {
            use tauri::Manager;
            crate::activity::game_stopped(&handle.state::<crate::Shared>(), pid);
            let _ = handle.emit(
                "process",
                json!({"id":item_id,"running":false,"exitCode":result.ok().and_then(|x|x.code())}),
            );
        }
    });
    if let Some(app) = app {
        post_launch(app, s);
    }
    Ok(json!({"pid":pid,"running":true}))
}
