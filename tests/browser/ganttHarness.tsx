import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";

import { GanttTreegrid } from "../../src/gantt/GanttTreegrid";
import {
  createGanttVerificationReadModel,
  reorderFirstVerificationActivity,
} from "../../src/gantt/verificationFixture";
import type { GanttReadModel } from "../../src/types/gantt";
import "../../src/styles.css";

function BrowserContract() {
  const [readModel, setReadModel] = useState<GanttReadModel>(
    createGanttVerificationReadModel,
  );

  useEffect(() => {
    const reorder = () =>
      setReadModel((current) => reorderFirstVerificationActivity(current));
    window.addEventListener("gantt-test-reorder", reorder);
    return () => window.removeEventListener("gantt-test-reorder", reorder);
  }, []);

  return <GanttTreegrid readModel={readModel} viewportHeight={480} todayDate="2026-08-23" />;
}

const root = document.getElementById("root");
if (!root) throw new Error("Browser contract root is missing");

createRoot(root).render(<BrowserContract />);
