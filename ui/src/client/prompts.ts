import { count, keys, object } from './validation.ts';
export type PromptContent =
  | {kind:'connection'; extension:string; launcher:string; category:string}
  | {kind:'destination'; localSaved:boolean; targets:string[]}
  | {kind:'quit-recording'};
export type PromptView = {generation:number; content:PromptContent};
export interface PromptClient { snapshot():Promise<PromptView>; reply(generation:number, choice:number):Promise<void>; dismiss():Promise<void> }
function label(raw:unknown):string {
  if (typeof raw !== 'string' || raw.length > 256 || /[\u0000-\u001f\u007f]/.test(raw)) throw new Error('Invalid prompt label');
  return raw;
}
export function parsePromptView(raw:unknown):PromptView {
  const value = object(raw); keys(value, ['generation', 'content']);
  const generation = count(value.generation);
  if (!generation) throw new Error('Invalid prompt generation');
  const content = object(value.content);
  if (content.kind === 'connection') {
    keys(content, ['kind','extension','launcher','category']);
    return {generation, content:{kind:'connection', extension:label(content.extension), launcher:label(content.launcher), category:label(content.category)}};
  }
  if (content.kind === 'destination') {
    keys(content, ['kind','localSaved','targets']);
    if (typeof content.localSaved !== 'boolean' || !Array.isArray(content.targets) || content.targets.length > 16) throw new Error('Invalid destination prompt');
    return {generation, content:{kind:'destination', localSaved:content.localSaved, targets:content.targets.map(label)}};
  }
  if (content.kind === 'quit-recording') { keys(content, ['kind']); return {generation, content:{kind:'quit-recording'}}; }
  throw new Error('Invalid prompt kind');
}
