import { createContext, useContext } from 'react';
import type { Localizer } from './formatter';
export const TextContext = createContext<Localizer>({ locale: 'en-US', t: (key) => key });
export const useText = () => useContext(TextContext);
