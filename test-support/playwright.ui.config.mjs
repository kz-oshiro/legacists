import {defineConfig} from '../apps/desktop/node_modules/@playwright/test/index.mjs';
import {resolve} from 'node:path';
const output=process.env.LEGACISTS_RUN_DIR??resolve('outputs/ui');
export default defineConfig({testDir:resolve('test-support/ui'),workers:2,retries:0,timeout:30000,reporter:[['list'],['json',{outputFile:resolve(output,'ui-results.json')}]],outputDir:resolve(output,'ui-work'),use:{baseURL:'http://127.0.0.1:1420',headless:true,viewport:{width:1440,height:900},screenshot:'off',video:'off',trace:'off'},webServer:{command:`"${process.execPath}" "${resolve('apps/desktop/node_modules/vite/bin/vite.js')}" --host 127.0.0.1 --port 1420 --strictPort`,cwd:resolve('apps/desktop'),url:'http://127.0.0.1:1420',reuseExistingServer:false,timeout:30000}});
