import {readFile} from 'node:fs/promises';
const packageJson=JSON.parse(await readFile(new URL('../apps/desktop/package.json',import.meta.url),'utf8'));
if(process.version!==`v${packageJson.engines.node}`)throw Error(`Expected Node ${packageJson.engines.node}; got ${process.version}`);
console.log(`Node ${process.version}; npm is invoked through the fixed local distribution.`);
