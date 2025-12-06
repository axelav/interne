import packageData from '../../package.json'
import styles from '../styles/Footer.module.css'

export default function Footer() {
  return (
    <footer className={styles.footer}>
      <div>v{packageData.version}</div>
      <a href="/data">Import/Export Data</a>
      <a href="https://honkytonk.in" target="_blank" rel="noopener noreferrer">
        Powered by honkytonkin'
      </a>
    </footer>
  )
}
