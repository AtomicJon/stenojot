import { BrowserRouter, Routes, Route } from "react-router-dom";
import { Layout } from "./components/Layout";
import { RecordingProvider } from "./hooks/useRecording";
import { RecordingPage } from "./pages/RecordingPage";
import { SettingsPage } from "./pages/SettingsPage";
import { MeetingsPage } from "./pages/MeetingsPage";
import "./global.scss";

function App() {
  return (
    <BrowserRouter>
      <RecordingProvider>
        <Routes>
          <Route element={<Layout />}>
            <Route path="/" element={<RecordingPage />} />
            <Route path="/meetings" element={<MeetingsPage />} />
            <Route path="/settings" element={<SettingsPage />} />
          </Route>
        </Routes>
      </RecordingProvider>
    </BrowserRouter>
  );
}

export default App;
