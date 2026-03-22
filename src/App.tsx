import { BrowserRouter, Routes, Route } from "react-router-dom";
import { Layout } from "./components/Layout";
import { RecordingPage } from "./pages/RecordingPage";
import "./global.scss";

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route element={<Layout />}>
          <Route path="/" element={<RecordingPage />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}

export default App;
