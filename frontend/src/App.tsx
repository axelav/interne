import { useState, useEffect, useMemo } from "react";
import orderBy from "lodash.orderby";
import AuthProvider from "./components/AuthProvider";
import CreateEntryForm from "./components/CreateEntryForm";
import Header from "./components/Header";
import Footer from "./components/Footer";
import {
  useEntries,
  useCreateEntry,
  useUpdateEntry,
  useDeleteEntry,
} from "./hooks/useEntries";
import { getAvailableAtPlusEntropy } from "./utils/entropy";
import { getRelativeTimeFromNow } from "./utils/date";
import { MODES, KEY_CODES } from "./utils/constants";
import type { Entry, CreateEntryInput } from "./types/entry";
import pageStyles from "./styles/Pages.module.css";
import styles from "./styles/Index.module.css";

const msgs = [
  {
    en: "Read a book!",
    eo: "Legi libron!",
  },
  {
    en: "Go outside!",
    eo: "Iru eksteren!",
  },
];

function AppContent() {
  const { data: entries = [], isLoading } = useEntries();
  const createEntry = useCreateEntry();
  const updateEntry = useUpdateEntry();
  const deleteEntry = useDeleteEntry();

  const [entry, setEntry] = useState<Entry | null>(null);
  const [mode, setMode] = useState<typeof MODES.VIEW | typeof MODES.EDIT>(
    MODES.VIEW,
  );
  const [isFilterActive, setIsFilterActive] = useState(true);
  const [searchText, setSearchText] = useState("");

  const emptyListMsg = msgs[1];

  // Compute visible/nextAvailable for each entry
  const entriesWithComputed = useMemo(() => {
    return entries.map((entry) => {
      const { nextAvailable, diff } = getAvailableAtPlusEntropy(entry);
      const visible = diff < 0;
      return { ...entry, visible, nextAvailable: nextAvailable.toDate() };
    });
  }, [entries]);

  // Filter and sort entries
  const visibleEntries = useMemo(() => {
    const filtered = entriesWithComputed.filter((x) => {
      if (searchText) {
        const escapeRegExp = (str: string) =>
          str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
        const regex = new RegExp(escapeRegExp(searchText), "gi");
        return (
          x.title?.match(regex) ||
          x.description?.match(regex) ||
          x.url?.match(regex)
        );
      } else {
        return isFilterActive ? x.visible : true;
      }
    });

    return orderBy(
      filtered,
      isFilterActive ? ["dismissed"] : ["dismissed", "nextAvailable"],
      isFilterActive ? ["desc"] : ["desc", "asc"],
    );
  }, [entriesWithComputed, isFilterActive, searchText]);

  useEffect(() => {
    const handleKeydown = (e: KeyboardEvent) => {
      if (e.keyCode === KEY_CODES.ESC) {
        if (mode === MODES.EDIT) {
          setMode(MODES.VIEW);
        } else if (document.activeElement === document.body) {
          setIsFilterActive(!isFilterActive);
        }
      }
    };

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [isFilterActive, mode]);

  const handleEntryClick = (entry: Entry) => {
    setTimeout(() => {
      updateEntry.mutate({
        id: entry.id,
        updates: {
          dismissed: new Date().toISOString(),
          visited: (entry.visited || 0) + 1,
        },
      });
    }, 200);
  };

  const handleViewFilterClick = () => setIsFilterActive(!isFilterActive);

  const handleSaveEntry = (input: CreateEntryInput) => {
    if (entry?.id) {
      updateEntry.mutate(
        { id: entry.id, updates: input },
        { onSuccess: () => setMode(MODES.VIEW) },
      );
    } else {
      createEntry.mutate(input, { onSuccess: () => setMode(MODES.VIEW) });
    }
  };

  const handleEditEntry = (entry: Entry) => {
    setEntry(entry);
    setMode(MODES.EDIT);
    window.scrollTo(0, 0);
  };

  const handleDeleteEntry = (entry: Entry) => {
    const shouldDelete = window.confirm("Are you sure?");
    if (shouldDelete) {
      deleteEntry.mutate(entry.id);
    }
  };

  if (isLoading) {
    return <div className={pageStyles.container}>Loading...</div>;
  }

  return (
    <div className={pageStyles.container}>
      <Header
        mode={mode}
        setMode={setMode}
        setEntry={setEntry}
        searchText={searchText}
        setSearchText={setSearchText}
      />

      <main className={pageStyles.main}>
        {mode === MODES.EDIT ? (
          <CreateEntryForm
            onSubmit={handleSaveEntry}
            entries={entries}
            {...(entry && {
              id: entry.id,
              url: entry.url,
              title: entry.title,
              description: entry.description,
              duration: entry.duration,
              interval: entry.interval,
              dismissed: entry.dismissed,
            })}
          />
        ) : (
          <div className={styles.grid}>
            {visibleEntries.length > 0 ? (
              visibleEntries.map((x) => (
                <div
                  key={x.id}
                  className={
                    x.visible
                      ? styles.card
                      : `${styles.card} ${styles.unavailable}`
                  }
                >
                  <div className={styles.viewed}>
                    <span>
                      {x.dismissed
                        ? `Last viewed ${getRelativeTimeFromNow(x.dismissed)}`
                        : "Never viewed"}
                    </span>
                  </div>
                  <a
                    href={x.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    onClick={() => handleEntryClick(x)}
                  >
                    <div className={styles.title}>
                      <h2 title={x.title}>{x.title}</h2>
                      <div className={styles.rarr}>&rarr;</div>
                    </div>
                    <p title={x.description || ""}>{x.description}</p>
                  </a>

                  <div className={styles["flex-between"]}>
                    <div className={styles.availability}>
                      {!x.visible && x.nextAvailable && (
                        <span>
                          Available{" "}
                          {getRelativeTimeFromNow(
                            x.nextAvailable.toISOString(),
                          )}
                        </span>
                      )}
                    </div>

                    <div className={styles.controls}>
                      {x.visible && (
                        <div
                          className={styles.ignore}
                          onClick={() => handleEntryClick(x)}
                        >
                          Mark Read
                        </div>
                      )}
                      <div
                        className={styles.edit}
                        onClick={() => handleEditEntry(x)}
                      >
                        Edit
                      </div>
                      <div
                        className={styles.delete}
                        onClick={() => handleDeleteEntry(x)}
                      >
                        Delete
                      </div>
                    </div>
                  </div>
                </div>
              ))
            ) : (
              <p
                className={styles.empty}
                title={searchText ? "Neniuj rezultoj" : emptyListMsg.eo}
              >
                {searchText ? "No results" : emptyListMsg.en}
              </p>
            )}
          </div>
        )}
      </main>

      {mode === MODES.VIEW && (
        <div
          className={styles.filter}
          onClick={handleViewFilterClick}
          style={{
            left: isFilterActive ? "-14px" : "-41px",
          }}
        >
          {isFilterActive ? "View All" : <span>View Available</span>}
        </div>
      )}

      <Footer />
    </div>
  );
}

export default function App() {
  return (
    <AuthProvider>
      <AppContent />
    </AuthProvider>
  );
}
