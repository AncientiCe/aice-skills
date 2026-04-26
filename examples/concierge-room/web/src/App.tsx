import { useEffect, useReducer } from "react";
import { connectViz } from "./lib/wsClient";
import type { VizEvent } from "./lib/types";
import StateBanner from "./components/StateBanner";
import TranscriptPane from "./components/TranscriptPane";
import ClassifierCard from "./components/ClassifierCard";
import McpCallCard from "./components/McpCallCard";
import AssistantReply from "./components/AssistantReply";
import JourneyTimeline from "./components/JourneyTimeline";
import HistoryDrawer from "./components/HistoryDrawer";
import { initialState, reducer } from "./state";

export default function App() {
  const [state, dispatch] = useReducer(reducer, initialState);

  useEffect(() => {
    const stop = connectViz(
      (ev: VizEvent) => dispatch({ type: "viz", event: ev }),
      (connected) => dispatch({ type: "status", connected }),
    );
    return stop;
  }, []);

  const activeTurn = state.activeTurnId
    ? state.turns[state.activeTurnId]
    : undefined;

  return (
    <div className="grid h-screen gap-4 p-4 lg:grid-cols-[minmax(360px,1fr)_minmax(420px,1.4fr)]">
      <div className="col-span-full">
        <StateBanner
          label={state.bridgeState}
          detail={state.bridgeDetail}
          connected={state.wsConnected}
        />
      </div>
      <div className="flex min-h-0 flex-col gap-4">
        <TranscriptPane turn={activeTurn} />
        <ClassifierCard turn={activeTurn} />
        <McpCallCard turn={activeTurn} />
        <AssistantReply turn={activeTurn} />
      </div>
      <div className="flex min-h-0 flex-col gap-4 overflow-hidden">
        <JourneyTimeline turns={state.turnOrder.map((id) => state.turns[id]).filter(Boolean)} />
        <HistoryDrawer turns={state.turnOrder.map((id) => state.turns[id]).filter(Boolean)} />
      </div>
    </div>
  );
}
