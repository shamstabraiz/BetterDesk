/**
 * App — root component
 * Checks auth state and renders Login or Layout.
 */
import { Show, onMount } from 'solid-js';
import { isLoggedIn, isLoading, initAuth } from './stores/auth';
import Login from './components/Login';
import Layout from './components/Layout';

export default function App() {
    onMount(() => {
        initAuth();
    });

    return (
        <Show when={!isLoading()} fallback={<Splash />}>
            <Show when={isLoggedIn()} fallback={<Login />}>
                <Layout />
            </Show>
        </Show>
    );
}

/** Minimal splash while checking auth */
function Splash() {
    return (
        <div class="loading-center">
            <div class="spinner" />
        </div>
    );
}
