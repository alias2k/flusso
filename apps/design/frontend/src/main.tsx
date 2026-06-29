import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { I18nProvider } from "./i18n";
import { TooltipProvider } from "@/components/ui/tooltip";
import "./index.css";

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <I18nProvider>
        <TooltipProvider delayDuration={300}>
          <App />
        </TooltipProvider>
      </I18nProvider>
    </StrictMode>,
  );
}
