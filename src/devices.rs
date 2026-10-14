//! Linux block devices via lsblk, with UDisks2/polkit-backed mount actions.
//! No root processes or GUI-specific dependencies are required.
use serde::Deserialize;
use std::{path::{Path,PathBuf},process::Command};

#[derive(Clone,Debug,PartialEq)]
pub struct Device {
    pub label:String,
    pub block:PathBuf,
    pub mount:Option<PathBuf>,
}
#[derive(Deserialize)]
struct Listing {blockdevices:Vec<Block>}
#[derive(Deserialize)]
struct Block {
    path:Option<String>,label:Option<String>,
    #[serde(rename="type")] kind:Option<String>,
    mountpoint:Option<String>,
    #[serde(default)] hotplug:serde_json::Value,
    #[serde(default)] rm:serde_json::Value,
}
fn enabled(v:&serde_json::Value)->bool{v.as_bool().unwrap_or(false)||v.as_i64().unwrap_or(0)!=0}
pub fn parse(text:&str)->Vec<Device>{
    let Ok(list)=serde_json::from_str::<Listing>(text) else{return Vec::new();};
    list.blockdevices.into_iter().filter_map(|b|{
        let block=PathBuf::from(b.path?);
        if !block.starts_with("/dev/"){return None;}
        let mount=b.mountpoint.filter(|m|!m.is_empty()).map(PathBuf::from);
        let user_mount=mount.as_ref().is_some_and(|p|{
            p.starts_with("/run/media/")||p.starts_with("/media/")||p.starts_with("/mnt/")
        });
        let removable=enabled(&b.hotplug)||enabled(&b.rm);
        if !user_mount && !removable {return None;}
        if !matches!(b.kind.as_deref(),Some("part")|Some("disk")|Some("crypt")){return None;}
        let label=b.label.filter(|x|!x.is_empty()).unwrap_or_else(||{
            block.file_name().unwrap_or_default().to_string_lossy().to_string()
        });
        Some(Device{label,block,mount})
    }).collect()
}
pub fn list()->Vec<Device>{
    let Ok(out)=Command::new("lsblk").args(["--json","--list","--output","PATH,LABEL,TYPE,MOUNTPOINT,HOTPLUG,RM"]).output() else {
        return Vec::new();
    };
    if !out.status.success(){return Vec::new();}
    parse(&String::from_utf8_lossy(&out.stdout))
}
pub fn mount(block:&Path)->Result<PathBuf,String>{
    run(block,"mount")?;
    list().into_iter().find(|d|d.block==block).and_then(|d|d.mount)
        .ok_or_else(||"Device mounted but mount point not available yet".into())
}
pub fn unmount(block:&Path)->Result<(),String>{run(block,"unmount")}
pub fn eject(block:&Path)->Result<(),String>{run(block,"power-off")}
fn run(block:&Path,action:&str)->Result<(),String>{
    if !block.starts_with("/dev/") || !["mount","unmount","power-off"].contains(&action){
        return Err("Invalid device/action".into());
    }
    let output=Command::new("udisksctl").arg(action).arg("-b").arg(block).output()
        .map_err(|e|format!("udisksctl: {e}"))?;
    if output.status.success(){Ok(())}else{Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())}
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn removable_discovery(){
        let result=parse(r#"{"blockdevices":[
           {"path":"/dev/sda1","label":"system","type":"part","mountpoint":"/","rm":false,"hotplug":false},
           {"path":"/dev/sdb1","label":"CAMERA","type":"part","mountpoint":null,"rm":true,"hotplug":true},
           {"path":"/dev/sdc1","label":"work","type":"part","mountpoint":"/mnt/work","rm":false,"hotplug":false}
        ]}"#);
        assert_eq!(result.len(),2);
        assert_eq!(result[0].label,"CAMERA");
        assert!(result[0].mount.is_none());
    }
}
