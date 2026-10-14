//! Best-effort Pinterest public-pin importer. It never bypasses authentication.
use serde::{Serialize,Deserialize};
use std::{fs,io::{self,Read},path::{Path,PathBuf},process::Command};
#[derive(Deserialize)]
struct Request { api_version:u32,action:String,directory:PathBuf,arguments:String }
#[derive(Serialize)]
struct Update { path:PathBuf,title:String,caption:String,source:String }
#[derive(Serialize)]
struct Response { message:String,refresh:bool,updates:Vec<Update> }

fn fetch(url:&str,maxsize:&str)->Result<Vec<u8>,String>{
    let output=Command::new("curl").args(["--fail","--silent","--show-error","--location",
       "--proto","=https","--proto-redir","=https","--connect-timeout","12",
       "--max-time","45","--max-filesize",maxsize,"--user-agent",
       "Mozilla/5.0 (compatible; Explorer/0.1)"]).arg(url).output().map_err(|e|e.to_string())?;
    if output.status.success(){Ok(output.stdout)}else{Err(String::from_utf8_lossy(&output.stderr).to_string())}
}
fn html_unescape(x:&str)->String{x.replace("&amp;","&").replace("&quot;","\"").replace("&#39;","'").replace("&lt;","<").replace("&gt;",">")}
fn meta(html:&str,property:&str)->Option<String> {
    for tag in html.split("<meta").skip(1).map(|s|s.split('>').next().unwrap_or("")) {
        if !tag.contains(property){continue;}
        let lower=tag.to_lowercase();
        let idx=lower.find("content=")?;
        let part=tag.get(idx+8..)?;
        let quote=part.chars().next()?;
        if quote!='\"'&&quote!='\''{continue;}
        let end=part[1..].find(quote)?;
        return Some(html_unescape(&part[1..1+end]));
    }
    None
}
fn valid_pin(url:&str)->bool {
    url.starts_with("https://www.pinterest.com/pin/") ||
    url.starts_with("https://pinterest.com/pin/") ||
    url.starts_with("https://pin.it/")
}
fn name(url:&str)->String {
    let pin_id=url.split("/pin/").nth(1).unwrap_or(url).split('/').next().unwrap_or("pin");
    let id=pin_id.chars().filter(|c|c.is_ascii_alphanumeric()).take(64).collect::<String>();
    if id.is_empty(){"pinterest".into()}else{id}
}
fn save_unique(root:&Path,stem:&str,bytes:&[u8])->Result<PathBuf,String>{
    let ext=if bytes.starts_with(b"\x89PNG"){ "png" }
        else if bytes.starts_with(b"RIFF"){ "webp" }
        else if bytes.starts_with(&[0xff,0xd8]){"jpg"}
        else{return Err("Unsupported or invalid image response".into());};
    for n in 0..10000{
        let file=if n==0{format!("{stem}.{ext}")}else{format!("{stem}-{n}.{ext}")};
        let path=root.join(file);
        let opened=fs::OpenOptions::new().write(true).create_new(true).open(&path);
        match opened {
            Ok(mut out)=>{use std::io::Write;out.write_all(bytes).map_err(|e|e.to_string())?;return Ok(path);}
            Err(e) if e.kind()==io::ErrorKind::AlreadyExists=>continue,
            Err(e)=>return Err(e.to_string())
        }
    }
    Err("Too many duplicate filenames".into())
}
fn run()->Result<(),String>{
    let mut input=String::new();io::stdin().read_to_string(&mut input).map_err(|e|e.to_string())?;
    let request:Request=serde_json::from_str(&input).map_err(|e|e.to_string())?;
    if request.api_version!=1||request.action!="run"{return Err("Invalid API protocol".into());}
    let arg=request.arguments.trim();
    if arg.is_empty(){return Err("Usage: :script pinterest_import PIN_URL | pins.txt".into());}
    let urls=if arg.starts_with("https://"){vec![arg.to_owned()]}
    else {
        let list=PathBuf::from(arg);
        let file=if list.is_absolute(){list}else{request.directory.join(list)};
        fs::read_to_string(file).map_err(|e|e.to_string())?.lines().map(str::trim)
            .filter(|s|!s.is_empty()&&!s.starts_with('#')).map(str::to_owned).take(2000).collect::<Vec<_>>()
    };
    let mut updates=Vec::new();let mut failures=Vec::new();
    for url in urls {
        if !valid_pin(&url){failures.push(format!("Invalid public pin URL: {url}"));continue;}
        let downloaded=(||->Result<Update,String>{
            let body=fetch(&url,"4000000")?;
            let html=String::from_utf8_lossy(&body);
            let image=meta(&html,"og:image").ok_or("No og:image on public pin page")?;
            if !(image.starts_with("https://i.pinimg.com/")||image.starts_with("https://www.pinterest.com/")) {
                return Err("Refusing non-Pinterest media URL".into());
            }
            let bytes=fetch(&image,"40000000")?;
            let file=save_unique(&request.directory,&name(&url),&bytes)?;
            Ok(Update{path:file,title:meta(&html,"og:title").unwrap_or_default(),
                caption:meta(&html,"og:description").unwrap_or_default(),source:url.clone()})
        })();
        match downloaded{Ok(u)=>updates.push(u),Err(e)=>failures.push(format!("{url}: {e}"))}
    }
    let count=updates.len();
    let message=format!("Imported {count} public pins; {} failed. {}",failures.len(),
        failures.first().cloned().unwrap_or_default());
    println!("{}",serde_json::to_string(&Response{message,refresh:true,updates}).map_err(|e|e.to_string())?);
    Ok(())
}
fn main(){if let Err(e)=run(){eprintln!("{e}");std::process::exit(1)}}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn parses_og_meta() {
        let html=r#"<meta property="og:image" content="https://i.pinimg.com/a.jpg"><meta property="og:title" content="Title &amp; art">"#;
        assert_eq!(meta(html,"og:title").as_deref(),Some("Title & art"));
        assert!(valid_pin("https://www.pinterest.com/pin/1234/"));
        assert!(!valid_pin("https://127.0.0.1/pin/123/"));
    }
}
