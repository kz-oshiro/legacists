import {readFile,readdir} from 'node:fs/promises';
import {join,resolve} from 'node:path';
const root=resolve(import.meta.dirname,'..');
for(const path of ['README.md','AGENTS.md','docs/requirements.md','docs/design.md','docs/development.md','docs/documentation.md']){const text=await readFile(join(root,path),'utf8');for(const [,link]of text.matchAll(/\]\(([^)]+)\)/g)){if(/^(https?:|#)/.test(link))continue;const target=resolve(join(root,path,'..'),link.split('#')[0]);try{await readFile(target);}catch{throw Error(`Broken document link: ${path} -> ${link}`);}}}
const config=JSON.parse(await readFile(join(root,'apps/desktop/src-tauri/tauri.conf.json'),'utf8'));
if(config.bundle.active||!config.build.windows.staticVCRuntime||!/object-src 'none'/.test(config.app.security.csp))throw Error('Packaging/security contract mismatch');
const source=await readFile(join(root,'crates/legacists-core/Cargo.toml'),'utf8');if(/tauri|vue|desktop/.test(source))throw Error('GUI dependency in Core');
console.log('Document links, Core boundary and packaging contracts passed.');
