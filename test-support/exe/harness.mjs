import {chromium} from '../../apps/desktop/node_modules/@playwright/test/index.mjs';
import {spawn,execFileSync} from 'node:child_process';
import {mkdir,readFile,cp,copyFile,writeFile} from 'node:fs/promises';
import {resolve,join} from 'node:path';
import {createServer} from 'node:net';
import {createHash} from 'node:crypto';
const root=resolve(import.meta.dirname,'../..');
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
function processes(){const result=execFileSync('pwsh',['-NoProfile','-Command',"@(Get-CimInstance Win32_Process | ForEach-Object {@{pid=$_.ProcessId;parent=$_.ParentProcessId;created=$_.CreationDate.ToFileTimeUtc().ToString()}})|ConvertTo-Json -Compress"],{encoding:'utf8'});return JSON.parse(result);}
function descendants(all,parent){const owned=[parent];for(let i=0;i<owned.length;i++){const p=owned[i];for(const child of all){if(child.parent===p.pid&&BigInt(child.created)>=BigInt(p.created)&&!owned.some(x=>x.pid===child.pid))owned.push(child);}}return owned;}
export async function start(testInfo,{existing,fixture}={}){
  const directory=testInfo.outputPath('case');await mkdir(directory,{recursive:true});
  const input=join(directory,'input');if(!existing)await cp(fixture??join(root,'crates/legacists-core/tests/fixtures/mixed'),input,{recursive:true});
  const original=process.env.LEGACISTS_EXISTING_EXE??process.env.LEGACISTS_EXE??join(root,'target/release/legacists-desktop.exe');
  const executable=existing?.executable??join(directory,'standalone/legacists-desktop.exe');if(!existing){await mkdir(resolve(executable,'..'),{recursive:true});await copyFile(original,executable);}
  const local=existing?.local??join(directory,'local');await mkdir(local,{recursive:true});
  const port=await new Promise(r=>{const server=createServer();server.listen(0,'127.0.0.1',()=>{const {port}=server.address();server.close(()=>r(port));});});
  const child=spawn(executable,[],{env:{...process.env,LOCALAPPDATA:local,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`},windowsHide:true,stdio:'ignore'});
  const parent=processes().find(p=>p.pid===child.pid);if(!parent){child.kill();throw Error('EXE process identity unavailable');}
  const creation=parent.created;
  let browser;const deadline=Date.now()+30000;while(Date.now()<deadline){try{browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);break;}catch{if(child.exitCode!==null)throw Error('EXE exited before CDP');await sleep(150);}}
  if(!browser){child.kill();throw Error('CDP connection unavailable');}
  const page=browser.contexts()[0].pages()[0],external=[];
  page.on('request',request=>{if(!/^(https?:\/\/tauri\.localhost\/|ipc:|https?:\/\/ipc\.localhost\/|data:|blob:)/.test(request.url()))external.push(request.url());});
  await page.getByRole('button',{name:'フォルダを開く',exact:true}).waitFor();
  let operation=0,stopped=false;
  async function dialog(action,value){
    const report=join(directory,`native-${child.pid}-${operation++}.json`);
    await new Promise((accept,reject)=>{
      const helper=spawn('pwsh',['-NoProfile','-STA','-File',join(root,'test-support/dialog.ps1'),'-TargetProcessId',String(child.pid),'-CreationTime',creation,'-Action',action,...(value?['-Value',value]:[]),'-ReportPath',report],{windowsHide:true});
      let output='';const timer=setTimeout(()=>{helper.kill();reject(Error('Owned native dialog helper timed out'));},30000);
      helper.stdout.on('data',c=>output+=c);helper.stderr.on('data',c=>output+=c);helper.on('exit',code=>{clearTimeout(timer);code===0?accept():reject(Error(output));});
    });
  }
  async function stop(){
    if(stopped)return;stopped=true;
    const owned=descendants(processes(),parent);
    await browser.close().catch(()=>{});child.kill();
    const deadline=Date.now()+15000;let alive;
    do{const all=processes();alive=owned.filter(p=>all.some(current=>current.pid===p.pid&&current.created===p.created));if(!alive.length)break;await sleep(150);}while(Date.now()<deadline);
    await writeFile(join(directory,`lifecycle-${child.pid}.json`),JSON.stringify({owned,alive,status:alive.length?'failed':'passed',method:'CDP disconnect and owned parent termination'},null,2));
    if(alive.length)throw Error('Owned EXE/WebView2 processes did not exit within bounded wait');
  }
  await writeFile(join(directory,`identity-${child.pid}.json`),JSON.stringify({pid:child.pid,creation,executable,sha256:createHash('sha256').update(await readFile(executable)).digest('hex'),webView2:browser.version(),networkObservation:'requests after CDP attachment'},null,2));
  return {page,child,creation,input,local,executable,directory,dialog,stop,external};
}
