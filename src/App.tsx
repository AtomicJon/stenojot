import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { Layout } from './components/Layout';
import { RecordingProvider } from './hooks/useRecording';
import { ToastProvider } from './components/Toast';
import { RecordingPage } from './pages/RecordingPage';
import { SettingsPage } from './pages/SettingsPage';
import { MeetingsPage } from './pages/MeetingsPage';
import './global.scss';

function App() {
  return (
    <BrowserRouter>
      <ToastProvider>
        <RecordingProvider>
          <Routes>
            <Route element={<Layout />}>
              <Route path="/" element={<RecordingPage />} />
              <Route path="/meetings" element={<MeetingsPage />} />
              <Route path="/settings" element={<SettingsPage />} />
            </Route>
          </Routes>
        </RecordingProvider>
      </ToastProvider>
    </BrowserRouter>
  );
}

export default App;
