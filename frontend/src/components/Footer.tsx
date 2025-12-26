import packageData from "../../package.json";
import styles from "../styles/Footer.module.css";

export default function Footer() {
  return (
    <footer className={styles.footer}>
      <div>v{packageData.version}</div>
      {/* TODO: we lost this bit when we added the db - it needs a refactor anyway */}
      {/* <a href="/data">Import/Export Data</a> */}
      <a href="https://honkytonk.in" target="_blank" rel="noopener noreferrer">
        Powered by honkytonkin'
      </a>
    </footer>
  );
}
