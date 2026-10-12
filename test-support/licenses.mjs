import {execFileSync} from 'node:child_process';
import {readFile,writeFile,readdir,mkdir} from 'node:fs/promises';
import {resolve,dirname,join} from 'node:path';
const root=resolve(import.meta.dirname,'..');
const metadata=JSON.parse(execFileSync('cargo',['metadata','--locked','--offline','--filter-platform','x86_64-pc-windows-msvc','--format-version','1'],{cwd:root,encoding:'utf8',maxBuffer:32*1024*1024}));
const lock=JSON.parse(await readFile(join(root,'apps/desktop/package-lock.json'),'utf8'));
const entries=[];
async function notice(directory,name,license,pkg){
  const files=(await readdir(directory)).filter(n=>/^(licen[cs]e|copying|notice|copyright)([-._]|$)/i.test(n));
  if(!files.length){const readme=(await readdir(directory)).find(n=>/^readme/i.test(n));if(readme){const text=await readFile(join(directory,readme),'utf8');if(/license|copyright/i.test(text))files.push(readme);}}
  if(!files.length){
    const target=join(root,'test-support/licenses',`${pkg.name}-${pkg.version}.txt`);
    let cached;try{cached=await readFile(target,'utf8');}catch{}
    if(!cached&&process.argv.includes('--fetch')&&pkg.repository?.startsWith('https://github.com/')){
      const vcs=JSON.parse(await readFile(join(directory,'.cargo_vcs_info.json'),'utf8'));
      const repo=pkg.repository.replace('https://github.com/','').replace(/\.git$/,'');
      for(const file of ['LICENSE','LICENSE-MIT','LICENSE-APACHE','LICENSE-MPL-2.0','LICENSE.txt','LICENSE.md','COPYING']){
        const url=`https://raw.githubusercontent.com/${repo}/${vcs.git.sha1}/${file}`;
        const response=await fetch(url);if(response.ok){const text=await response.text();if(text.length>100){cached=(cached??'')+`Source: ${url}\n\n${text}\n`;}}
      }
      if(cached){await mkdir(dirname(target),{recursive:true});await writeFile(target,cached);}
    }
    if(!cached&&license==='MPL-2.0'){
      const text=await readFile(join(root,'test-support/licenses/MPL-2.0.txt'),'utf8');
      cached=`Unmodified source is available under MPL-2.0 at https://crates.io/api/v1/crates/${pkg.name}/${pkg.version}/download\nRepository: ${pkg.repository}\nLicense text: https://www.mozilla.org/en-US/MPL/2.0/\n\n${text}`;
    }
    if(!cached)throw Error(`License text unavailable: ${name}`);
    entries.push(`===== ${name} (${license??'see text'}) =====\n${cached}\n`);return;
  }
  const body=[];for(const file of files.sort()){try{body.push(`${file}\n${await readFile(join(directory,file),'utf8')}`);}catch{}}
  if(!body.length)throw Error(`License files unreadable: ${name}`);
  entries.push(`===== ${name} (${license??'see text'}) =====\n${body.join('\n')}\n`);
}
const nodes=new Map(metadata.resolve.nodes.map(n=>[n.id,n]));const included=new Set();const queue=metadata.packages.filter(p=>p.name==='legacists-desktop').map(p=>p.id);while(queue.length){const id=queue.pop();if(included.has(id))continue;included.add(id);queue.push(...(nodes.get(id)?.dependencies??[]));}
for(const p of metadata.packages.filter(p=>p.source&&included.has(p.id))){await notice(dirname(p.manifest_path),`Rust: ${p.name}@${p.version}`,p.license,p);}
for(const [path,p]of Object.entries(lock.packages)){if(!path||p.dev||p.optional)continue;const directory=join(root,'apps/desktop',path);const json=JSON.parse(await readFile(join(directory,'package.json'),'utf8'));await notice(directory,`npm: ${json.name}@${json.version}`,json.license);}
const out=join(root,'apps/desktop/public/licenses.txt');await mkdir(dirname(out),{recursive:true});await writeFile(out,`legacists third-party licenses and notices\nGenerated from Cargo.lock and package-lock.json.\n\n${entries.sort().join('\n')}`);console.log(`Collected dependency notices: ${entries.length}`);
