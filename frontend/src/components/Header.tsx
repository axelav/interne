import { useState, useRef, useEffect } from "react";
import { CSSTransition } from "react-transition-group";
import { Input } from "./Forms";
import type { InputRef } from "./Forms";
import { getCurrentDateLocalized } from "../utils/date";
import { toTitleCase } from "../utils/formatters";
import { MODES, KEY_CODES } from "../utils/constants";
import packageData from "../../package.json";
import styles from "../styles/Header.module.css";

interface HeaderProps {
  mode: typeof MODES.VIEW | typeof MODES.EDIT;
  setMode: (mode: typeof MODES.VIEW | typeof MODES.EDIT) => void;
  setEntry: (entry: null) => void;
  searchText: string;
  setSearchText: (text: string) => void;
}

export default function Header({
  mode,
  setMode,
  setEntry,
  searchText,
  setSearchText,
}: HeaderProps) {
  const [showSearch, setShowSearch] = useState(false);
  const [showDate, setShowDate] = useState(true);
  const inputRef = useRef<InputRef>(null);

  useEffect(() => {
    const handleKeydown = (ev: KeyboardEvent) => {
      if (ev.keyCode === KEY_CODES.FWD_SLASH) {
        if (document.activeElement === document.body) {
          setShowSearch(true);
          inputRef.current?.focus();

          // prevent `/` character from being used as input value
          ev.preventDefault();
          ev.stopPropagation();
        }
      }

      if (ev.keyCode === KEY_CODES.ESC) {
        if (
          !!inputRef.current &&
          inputRef.current.className ===
            (document.activeElement as HTMLElement).className
        ) {
          setSearchText("");
          setShowSearch(false);
        }
      }
    };

    document.addEventListener("keydown", handleKeydown);

    return () => document.removeEventListener("keydown", handleKeydown);
  }, [searchText, setSearchText]);

  useEffect(() => {
    if (inputRef.current) {
      // FIXME there's a bug when you
      // 1. enter text in input
      // 2. blur input
      // 3. hit `/` to focus input again
      // 4. enter new text
      // 5. blur - input value remains but input is hidden
      inputRef.current.addEventListener("blur", () => {
        if (!searchText) {
          setShowSearch(false);
        }
      });
    }
  }, [searchText]);

  return (
    <header className={styles.header}>
      <h1 className={styles.title}>
        <a href="/">{toTitleCase(packageData.name)}</a>
      </h1>

      {!!mode && (
        <div
          className={styles.mode}
          onClick={() => {
            setEntry(null);

            switch (mode) {
              case MODES.EDIT:
                setMode(MODES.VIEW);
                break;
              case MODES.VIEW:
                setMode(MODES.EDIT);
                break;
              default:
                setMode(MODES.VIEW);
            }
          }}
        >
          {mode === MODES.EDIT ? "View Entries" : "Add Entry"}
        </div>
      )}

      <div
        className={styles.date}
        onMouseEnter={() => setShowSearch(true)}
        onMouseLeave={() => {
          if (!searchText) {
            setShowSearch(false);
          }
        }}
      >
        <CSSTransition
          in={showSearch}
          timeout={200}
          classNames="fade"
          unmountOnExit
          onEnter={() => {
            setShowDate(false);
            inputRef.current?.focus();
          }}
          onExited={() => setShowDate(true)}
        >
          <div>
            <Input
              ref={inputRef}
              value={searchText}
              onChange={setSearchText}
              placeholder="Search"
            />
          </div>
        </CSSTransition>

        <CSSTransition
          in={showDate}
          timeout={200}
          classNames="fade"
          unmountOnExit
          onEnter={() => setShowSearch(false)}
          onExited={() => setShowSearch(true)}
        >
          <div>{getCurrentDateLocalized()}</div>
        </CSSTransition>
      </div>
    </header>
  );
}
