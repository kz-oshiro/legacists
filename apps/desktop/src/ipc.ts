import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export interface Context { projectId?:string; snapshotId?:string }
export interface Progress {projectId:string;jobId:string;sequence:number;stage:string;completed?:number;total?:number;terminal?:boolean;snapshotId?:string;error?:string}
let sequence=0;
export async function request<T>(operation:string,args:unknown={},context:Context={}):Promise<T>{
  const requestId=`request-${++sequence}`;
  const response=await invoke<{requestId:string;data:T;error?:{code:string}}>('ipc',{request:{requestId,operation,...context,args}});
  if(response.requestId!==requestId)throw Error('INVALID_RESPONSE');
  if(response.error)throw Error(response.error.code);
  return response.data;
}
export const subscribe=(callback:(event:Progress)=>void)=>listen<Progress>('analysis-progress',event=>callback(event.payload));
