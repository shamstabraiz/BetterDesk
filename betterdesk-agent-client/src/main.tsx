/* @refresh reload */
import { render } from "solid-js/web";
import App from "./App";
import { frontendLog, installFrontendErrorLogging } from "./lib/logger";
import "./styles/global.css";

const root = document.getElementById("root");

installFrontendErrorLogging();
frontendLog("info", "app.main", "Rendering BetterDesk Agent root");

render(() => <App />, root!);
