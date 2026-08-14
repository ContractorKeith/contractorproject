import "@fontsource/barlow/latin-400.css";
import "@fontsource/barlow/latin-500.css";
import "@fontsource/barlow-condensed/latin-600.css";
import { createRoot } from "react-dom/client";

import { GanttPrototype } from "./GanttPrototype";
import "./prototype.css";

const root = document.getElementById("gantt-prototype-root");
if (!root) throw new Error("Gantt prototype root is missing");

document.documentElement.dataset.platform = /Mac/.test(navigator.platform) ? "macos" : "other";
createRoot(root).render(<GanttPrototype />);
