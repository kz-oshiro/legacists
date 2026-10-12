import {mkdir,readFile,writeFile} from 'node:fs/promises';
import {resolve,join} from 'node:path';
const directory=process.env.LEGACISTS_RUN_DIR??resolve('outputs/performance');
await mkdir(directory,{recursive:true});
let conditions;try{conditions=JSON.parse(await readFile(new URL('./performance/conditions.json',import.meta.url),'utf8'));}catch{}
const reason=conditions?.status==='approved'?'measurement-runner-pending-issue-6-contract':'issue-6-conditions-not-approved';
await writeFile(join(directory,'performance-results.json'),JSON.stringify({schemaVersion:1,status:'not_run',reason,success:false,conditionsId:conditions?.id??null},null,2));
console.error(`T-20 not run: ${reason}. See Issue #6 and docs/design.md.`);
process.exitCode=1;
