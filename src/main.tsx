import React from "react";
import ReactDOM from "react-dom/client";
import * as Sentry from "@sentry/react";
import App from "./App";
import "./styles/globals.css";

if (import.meta.env.VITE_SENTRY_DSN?.trim()) {
  Sentry.init({
    dsn: import.meta.env.VITE_SENTRY_DSN.trim(),
    enabled: true,
    debug: false,
    release: __APP_VERSION__,
    environment: import.meta.env.MODE,
  });
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
