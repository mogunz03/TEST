import React from "react";
import ReactDOM from "react-dom/client";
import "@/App.css";
import "@/i18n";
import { MeetingPill } from "./MeetingPill";

document.documentElement.dataset.theme = "dark";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <MeetingPill />
  </React.StrictMode>,
);
