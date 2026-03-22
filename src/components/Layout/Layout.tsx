import { Outlet } from "react-router-dom";
import s from "./Layout.module.scss";

/** App shell with consistent layout wrapping all routes. */
export function Layout() {
  return (
    <main className={s.layout}>
      <Outlet />
    </main>
  );
}
