import { createRoot } from 'react-dom/client';
import type { Root } from 'react-dom/client';
import { installInputInteractions } from './components/input-interactions';
import { App } from './app';
import './styles/tokens.css';
import './styles/components.css';
import './styles/settings.css';
import './styles/panel-window.css';
import './styles/sandbox.css';

const previous = import.meta.hot?.data.root as Root | undefined;
const element = document.getElementById('root')!;
const disposeInteractions = installInputInteractions(element);
const root = previous ?? createRoot(element);
root.render(<App />);
if (import.meta.hot) import.meta.hot.dispose((data) => { data.root = root; disposeInteractions(); });
