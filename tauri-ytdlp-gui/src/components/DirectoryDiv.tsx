// Ask user where to store the media
// Checklist of radio type items (radio input hidden under square button)'
// ./Downloads
// ├── YouTube
// │   └── videos (.mp4)
// │   └── audio (.mp3)
// ├── Instagram
// │   └── images (.png, .jpg)
// │   └── gifs (.gif)
// │   └── videos (.mp4)
// │   └── audio (.mp3)
// ├── X
// │   └── images (.png, .jpg)
// │   └── gifs (.gif)
// │   └── videos (.mp4)
// │   └── audio (.mp3)
import type { DirectorySelection, DirectoryTree } from "../types";
import "./DirectoryDiv.css";

interface DirectoryDivProps {
  value: DirectorySelection;
  onChange: (value: DirectorySelection) => void;
  /** Opens the native folder picker for the custom option. */
  onBrowse: () => void;
  /** Folders actually created by the backend for the detected platform. */
  tree?: DirectoryTree | null;
  disabled?: boolean;
}

// Extensions shown next to each type folder, matching the tree above.
const FOLDER_EXTENSIONS: Record<string, string> = {
  images: ".png, .jpg",
  gifs: ".gif",
  videos: ".mp4",
  audio: ".mp3",
};

/** Builds the ASCII tree lines: root, platform, then one line per folder. */
function buildTreeLines(tree: DirectoryTree): string[] {
  const folders = tree.folders.map((folder, index) => {
    const isLast = index === tree.folders.length - 1;
    const extension = FOLDER_EXTENSIONS[folder];
    const label = extension ? `${folder} (${extension})` : folder;
    return `   ${isLast ? "└─" : "├─"} ${label}`;
  });

  return [tree.root, `└─ ${tree.platform}`, ...folders];
}

export default function DirectoryDiv({
  value,
  onChange,
  onBrowse,
  tree,
  disabled = false,
}: DirectoryDivProps) {
  // The custom option puts every output type into one user-chosen folder.
  const isCustom = value.mode === "custom";

  return (
    <fieldset className="directory-div" disabled={disabled}>
      <legend className="directory-div__legend">Save to</legend>

      <div className="directory-div__options">
        <label className="directory-option">
          <input
            className="directory-option__input"
            type="radio"
            name="download-directory"
            value="default"
            checked={!isCustom}
            onChange={() => onChange({ mode: "default", path: value.path })}
          />
          <span className="directory-option__label">Default</span>
        </label>

        <label className="directory-option">
          <input
            className="directory-option__input"
            type="radio"
            name="download-directory"
            value="custom"
            checked={isCustom}
            onChange={() => onChange({ mode: "custom", path: value.path })}
          />
          <span className="directory-option__label">Custom…</span>
        </label>
      </div>

      {isCustom ? (
        <div className="directory-div__custom">
          <button
            type="button"
            className="directory-div__browse"
            onClick={onBrowse}
            disabled={disabled}
          >
            Choose folder…
          </button>
          <p
            className={`directory-div__path${
              value.path ? "" : " directory-div__path--empty"
            }`}
            title={value.path}
          >
            {value.path ?? "No folder selected yet."}
          </p>
        </div>
      ) : tree ? (
        // Shows the real folders the backend created for this platform.
        <pre className="directory-div__tree" title={tree.root}>
          {buildTreeLines(tree).join("\n")}
        </pre>
      ) : (
        // Before analysis no platform folder exists yet.
        <p className="directory-div__path">
          Folders are created after a URL is analyzed.
        </p>
      )}
    </fieldset>
  );
}
