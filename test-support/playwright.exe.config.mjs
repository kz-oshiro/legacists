import {defineConfig} from '../apps/desktop/node_modules/@playwright/test/index.mjs';
import {resolve} from 'node:path';
const output=process.env.LEGACISTS_RUN_DIR??resolve('outputs/exe');
export default defineConfig({testDir:resolve('test-support/exe'),workers:1,retries:0,timeout:90000,reporter:[['list'],['json',{outputFile:resolve(output,'exe-results.json')}]],outputDir:resolve(output,'exe-work'),use:{screenshot:'off',video:'off',trace:'off'}});
