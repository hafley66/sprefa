import type { Step } from "../../lib/scene/0_types"

const taskOpen = { key: "task-7-open", values: [7, "slides", "open"], weight: 1 } as const
const taskDone = { key: "task-7-done", values: [7, "slides", "done"], weight: 1 } as const

export const collisionGridSteps: readonly Step[] = [
  {
    caption: "One concrete update anchors the comparison: task 7 changes from open to done.",
    elements: [
      { kind: "zset", key: "before", x: 115, y: 175, model: { title: "before", rows: [taskOpen] } },
      { kind: "text", key: "arrow", x: 485, y: 220, text: "→", size: 34 },
      { kind: "zset", key: "after", x: 650, y: 175, model: { title: "after", rows: [taskDone] } },
    ],
    codeStep: 0,
  },
  {
    caption: "State names different stored values: a SQL table and Redux store hold the current task, while an RxJS stream emits values over time.",
    elements: [
      { kind: "zset", key: "sql", x: 55, y: 150, showWeights: false, model: { title: "SQL table now", rows: [taskDone] } },
      { kind: "zset", key: "redux", x: 385, y: 150, showWeights: false, model: { title: "Redux state now", rows: [taskDone] } },
      { kind: "text", key: "rxjs", x: 745, y: 170, text: "RxJS emissions", size: 23 },
      { kind: "text", key: "emissions", x: 735, y: 225, text: "false  →  true", size: 25 },
    ],
    codeStep: 1,
  },
  {
    caption: "DBSP and Differential Dataflow encode the update as weighted changes. Differential Dataflow also attaches a logical timestamp.",
    elements: [
      { kind: "delta", key: "dbsp", x: 105, y: 145, width: 350, model: { title: "DBSP Δ at tick 12", rows: [{ ...taskOpen, weight: -1 }, taskDone] } },
      { kind: "delta", key: "dd", x: 600, y: 145, width: 350, model: { title: "DD updates at t=12", rows: [{ ...taskOpen, weight: -1 }, taskDone] } },
      { kind: "text", key: "dd-record", x: 645, y: 355, text: "(data, time, diff)", size: 22 },
    ],
    codeStep: 2,
  },
  {
    caption: "The word time selects different coordinates: commit order, dispatch order, scheduler delivery, or logical progress.",
    elements: [
      { kind: "text", key: "sql", x: 90, y: 135, text: "SQL", size: 22 },
      { kind: "text", key: "sql-value", x: 245, y: 135, text: "transaction commits #41 then #42", size: 21 },
      { kind: "text", key: "redux", x: 90, y: 195, text: "Redux", size: 22 },
      { kind: "text", key: "redux-value", x: 245, y: 195, text: "dispatch sequence 41 then 42", size: 21 },
      { kind: "text", key: "rxjs", x: 90, y: 255, text: "RxJS", size: 22 },
      { kind: "text", key: "rxjs-value", x: 245, y: 255, text: "scheduler delivers false then true", size: 21 },
      { kind: "text", key: "dbsp", x: 90, y: 315, text: "DBSP", size: 22 },
      { kind: "text", key: "dbsp-value", x: 245, y: 315, text: "logical tick 12", size: 21 },
      { kind: "text", key: "dd", x: 90, y: 375, text: "DD", size: 22 },
      { kind: "text", key: "dd-value", x: 245, y: 375, text: "timestamp 12; frontier beyond 12", size: 21 },
    ],
    codeStep: 3,
  },
  {
    caption: "Join also collides. SQL, DBSP, and Differential Dataflow match keyed collections. Redux selectors and RxJS combineLatest have different update contracts.",
    elements: [
      { kind: "text", key: "relational", x: 85, y: 135, text: "SQL / DBSP / DD", size: 23 },
      { kind: "text", key: "relational-value", x: 400, y: 135, text: "tasks ⋈ owners on owner_id", size: 22 },
      { kind: "text", key: "redux", x: 85, y: 225, text: "Redux selector", size: 23 },
      { kind: "text", key: "redux-value", x: 400, y: 225, text: "reads two state slices when selected", size: 22 },
      { kind: "text", key: "rxjs", x: 85, y: 315, text: "RxJS combineLatest", size: 23 },
      { kind: "text", key: "rxjs-value", x: 400, y: 315, text: "pairs the latest emission from each stream", size: 22 },
    ],
    codeStep: 4,
  },
  {
    caption: "Retraction is explicit in weighted systems. SQL deletes a row. Redux replaces state. RxJS needs a stateful operator or an explicit domain event to remove prior data.",
    elements: [
      { kind: "text", key: "sql", x: 75, y: 130, text: "SQL", size: 22 },
      { kind: "text", key: "sql-value", x: 255, y: 130, text: "DELETE task 7", size: 22 },
      { kind: "text", key: "redux", x: 75, y: 200, text: "Redux", size: 22 },
      { kind: "text", key: "redux-value", x: 255, y: 200, text: "next state omits task 7", size: 22 },
      { kind: "text", key: "rxjs", x: 75, y: 270, text: "RxJS", size: 22 },
      { kind: "text", key: "rxjs-value", x: 255, y: 270, text: "taskRemoved(7), then scan", size: 22 },
      { kind: "text", key: "weighted", x: 75, y: 390, text: "DBSP / DD", size: 22 },
      { kind: "delta", key: "weighted-value", x: 450, y: 390, width: 340, model: { title: "weighted retraction", rows: [{ ...taskDone, weight: -1 }] } },
    ],
    codeStep: 5,
  },
  {
    caption: "Translate by operation and contract: current value, change record, time coordinate, keyed match, and removal behavior.",
    elements: [
      { kind: "text", key: "head", x: 70, y: 110, text: "word", size: 21 },
      { kind: "text", key: "sql", x: 240, y: 110, text: "SQL", size: 21 },
      { kind: "text", key: "dbsp-dd", x: 415, y: 110, text: "DBSP / DD", size: 21 },
      { kind: "text", key: "redux", x: 650, y: 110, text: "Redux", size: 21 },
      { kind: "text", key: "rxjs", x: 835, y: 110, text: "RxJS", size: 21 },
      { kind: "text", key: "change", x: 70, y: 185, text: "change", size: 21 },
      { kind: "text", key: "sql-change", x: 240, y: 185, text: "UPDATE", size: 20 },
      { kind: "text", key: "dbsp-change", x: 415, y: 185, text: "weighted Δ", size: 20 },
      { kind: "text", key: "redux-change", x: 650, y: 185, text: "action", size: 20 },
      { kind: "text", key: "rxjs-change", x: 835, y: 185, text: "emission", size: 20 },
      { kind: "text", key: "time", x: 70, y: 260, text: "time", size: 21 },
      { kind: "text", key: "sql-time", x: 240, y: 260, text: "commit", size: 20 },
      { kind: "text", key: "dbsp-time", x: 415, y: 260, text: "logical", size: 20 },
      { kind: "text", key: "redux-time", x: 650, y: 260, text: "dispatch", size: 20 },
      { kind: "text", key: "rxjs-time", x: 835, y: 260, text: "scheduler", size: 20 },
      { kind: "text", key: "remove", x: 70, y: 335, text: "remove", size: 21 },
      { kind: "text", key: "sql-remove", x: 240, y: 335, text: "DELETE", size: 20 },
      { kind: "text", key: "dbsp-remove", x: 415, y: 335, text: "diff −1", size: 20 },
      { kind: "text", key: "redux-remove", x: 650, y: 335, text: "new state", size: 20 },
      { kind: "text", key: "rxjs-remove", x: 835, y: 335, text: "model it", size: 20 },
    ],
    codeStep: 6,
  },
]
