/* @refresh reload */
import { render } from 'solid-js/web';
import { setLocale } from './lib/i18n';
import App from './App';
import './styles/global.css';

const root = document.getElementById('root');
if (!root) throw new Error('Root element not found');

// Load default locale then render
setLocale(localStorage.getItem('bd_mgmt_locale') || navigator.language || 'en')
    .then(() => render(() => <App />, root));
