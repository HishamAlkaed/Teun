import { Link, NavLink, Routes, Route, Navigate } from "react-router-dom";
import { EvalTab } from "./EvalTab";
import { QuestionsTab } from "./QuestionsTab";
import { FeedbackTab } from "./FeedbackTab";
import { DocsTab } from "./DocsTab";
import { SettingsPanel } from "../chat/components/SettingsPanel";

const tabs = [
  { path: "", label: "Evaluatie" },
  { path: "questions", label: "Vragen" },
  { path: "feedback", label: "Feedback" },
  { path: "docs", label: "Documentatie" },
  { path: "settings", label: "Instellingen" },
];

export function AdminLayout() {
  return (
    <div className="min-h-screen bg-page-bg flex flex-col">
      <header className="bg-surface border-b border-border">
        <div className="max-w-6xl mx-auto px-6 py-4 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Link to="/" className="text-sm font-semibold text-text-primary tracking-tight">
              Teun
            </Link>
            <span className="text-text-tertiary">/</span>
            <span className="text-sm font-medium text-text-secondary">Admin</span>
          </div>
          <Link
            to="/chat"
            className="text-xs text-accent hover:text-accent-dark transition-colors"
          >
            &larr; Terug naar chat
          </Link>
        </div>

        <div className="max-w-6xl mx-auto px-6">
          <nav className="flex gap-4">
            {tabs.map((tab) => (
              <NavLink
                key={tab.path}
                to={tab.path === "" ? "/admin" : `/admin/${tab.path}`}
                end={tab.path === ""}
                className={({ isActive }) =>
                  `pb-2 text-xs font-medium border-b-2 transition-colors ${
                    isActive
                      ? "border-accent text-accent"
                      : "border-transparent text-text-tertiary hover:text-text-secondary"
                  }`
                }
              >
                {tab.label}
              </NavLink>
            ))}
          </nav>
        </div>
      </header>

      <div className="flex-1">
        <Routes>
          <Route index element={<div className="max-w-6xl mx-auto px-6 py-6"><EvalTab /></div>} />
          <Route path="questions" element={<div className="max-w-6xl mx-auto px-6 py-6"><QuestionsTab /></div>} />
          <Route path="feedback" element={<div className="max-w-6xl mx-auto px-6 py-6"><FeedbackTab /></div>} />
          <Route path="docs" element={<div className="max-w-6xl mx-auto px-6 py-6"><DocsTab /></div>} />
          <Route path="settings" element={<div className="max-w-6xl mx-auto px-6 py-6"><SettingsPanel /></div>} />
          <Route path="*" element={<Navigate to="/admin" replace />} />
        </Routes>
      </div>
    </div>
  );
}
