document.addEventListener("DOMContentLoaded", function () {
  let selectedIndex = -1;
  const threadItems = document.querySelectorAll(".thread-item");

  function updateSelection() {
    threadItems.forEach((item, index) => {
      if (index === selectedIndex) {
        item.style.outline = "2px solid #007bff";
        item.style.outlineOffset = "2px";
        item.setAttribute("data-selected", "true");
      } else {
        item.style.outline = "";
        item.style.outlineOffset = "";
        item.removeAttribute("data-selected");
      }
    });

    if (selectedIndex >= 0 && selectedIndex < threadItems.length) {
      threadItems[selectedIndex].scrollIntoView({ behavior: "smooth", block: "nearest" });
    }
  }

  document.addEventListener("keydown", function (e) {
    if (e.target.tagName === "INPUT" || e.target.tagName === "TEXTAREA") {
      if (e.key === "Escape") {
        e.target.blur();
      }
      return;
    }

    switch (e.key) {
      case "j":
      case "ArrowDown":
        e.preventDefault();
        if (selectedIndex < threadItems.length - 1) {
          selectedIndex++;
          updateSelection();
        }
        break;
      case "k":
      case "ArrowUp":
        e.preventDefault();
        if (selectedIndex > 0) {
          selectedIndex--;
          updateSelection();
        }
        break;
      case "Enter":
        if (selectedIndex >= 0 && selectedIndex < threadItems.length) {
          e.preventDefault();
          threadItems[selectedIndex].click();
        }
        break;
      case "r":
        const replyTextarea = document.querySelector("#text");
        if (replyTextarea) {
          e.preventDefault();
          replyTextarea.focus();
        }
        break;
      case "?":
        e.preventDefault();
        showKeyboardHelp();
        break;
      case "Escape":
        const dialog = document.querySelector("dialog[open]");
        if (dialog) {
          dialog.close();
        }
        break;
    }
  });

  function showKeyboardHelp() {
    let existingDialog = document.getElementById("keyboard-help-dialog");
    if (existingDialog) {
      existingDialog.close();
      existingDialog.remove();
    }

    const dialog = document.createElement("dialog");
    dialog.id = "keyboard-help-dialog";
    dialog.style.padding = "2rem";
    dialog.style.border = "1px solid #ddd";
    dialog.style.borderRadius = "0.5rem";
    dialog.style.maxWidth = "500px";

    dialog.innerHTML = `
            <h3>Keyboard Shortcuts</h3>
            <table style="width: 100%; border-collapse: collapse;">
                <tr><td style="padding: 0.5rem;"><kbd>j</kbd> or <kbd>↓</kbd></td><td>Navigate down</td></tr>
                <tr><td style="padding: 0.5rem;"><kbd>k</kbd> or <kbd>↑</kbd></td><td>Navigate up</td></tr>
                <tr><td style="padding: 0.5rem;"><kbd>Enter</kbd></td><td>Open selected thread</td></tr>
                <tr><td style="padding: 0.5rem;"><kbd>r</kbd></td><td>Focus reply textarea</td></tr>
                <tr><td style="padding: 0.5rem;"><kbd>Escape</kbd></td><td>Clear focus / Close dialogs</td></tr>
                <tr><td style="padding: 0.5rem;"><kbd>?</kbd></td><td>Show this help</td></tr>
            </table>
            <button onclick="this.closest('dialog').close()" style="margin-top: 1rem;">Close</button>
        `;

    document.body.appendChild(dialog);
    dialog.showModal();
  }
});
