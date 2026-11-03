import { object, keys } from './validation.ts';
export type PanelFrameRequest = {
  first:boolean; pixelRatio:number; width:number; height:number; feedbackHeight:number; overlay:boolean; modal:boolean; left:number; top:number;
};
export type PanelInsets = {left:number; top:number; feedbackAbove:boolean};
export interface PanelFrameClient {
  layout(request:PanelFrameRequest):Promise<PanelInsets>;
  drag():Promise<void>;
}
export function parsePanelInsets(raw:unknown):PanelInsets {
  const value = object(raw); keys(value, ['left','top','feedbackAbove']);
  if (typeof value.left !== 'number' || !Number.isFinite(value.left) || value.left < 0 || value.left > 768 ||
      typeof value.top !== 'number' || !Number.isFinite(value.top) || value.top < 0 || value.top > 768 ||
      typeof value.feedbackAbove !== 'boolean') throw new Error('Invalid panel frame response');
  return {left:value.left, top:value.top, feedbackAbove:value.feedbackAbove};
}
