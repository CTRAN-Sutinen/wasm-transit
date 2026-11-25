/// Arrow-key movement for a group of `role="radio"` or `role="tab"` buttons. Put
/// the action on the group's element. The group must give the selected item
/// `tabindex="0"` and the others `tabindex="-1"`, so Tab enters the group once.
/// An arrow key, Home, or End focuses the next item and selects it, as a native
/// radio group does.
export function roving(group: HTMLElement) {
  const onkey = (e: KeyboardEvent) => {
    const items = [...group.querySelectorAll<HTMLElement>('[role="radio"], [role="tab"]')];
    const at = items.indexOf(document.activeElement as HTMLElement);
    if (at < 0) return;
    let to: number;
    if (e.key === "ArrowRight" || e.key === "ArrowDown") to = (at + 1) % items.length;
    else if (e.key === "ArrowLeft" || e.key === "ArrowUp") to = (at - 1 + items.length) % items.length;
    else if (e.key === "Home") to = 0;
    else if (e.key === "End") to = items.length - 1;
    else return;
    e.preventDefault();
    items[to].focus();
    items[to].click();
  };
  group.addEventListener("keydown", onkey);
  return { destroy: () => group.removeEventListener("keydown", onkey) };
}
