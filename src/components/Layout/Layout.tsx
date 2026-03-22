import { Link, Outlet } from "react-router-dom";
import s from "./Layout.module.scss";

/** App shell with top navigation and consistent layout wrapping all routes. */
export function Layout() {
  return (
    <div className={s.shell}>
      <nav className={s.nav}>
        <Link to="/" className={s.navBrand}>
          EchoNotes
        </Link>
        <Link to="/settings" className={s.navLink}>
          Settings
        </Link>
      </nav>
      <main className={s.layout}>
        <Outlet />
      </main>
    </div>
  );
}
