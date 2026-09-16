import { formatTeam } from "./format";
import { TeamRecord } from "./records";

export function reportTeam(team: TeamRecord): string {
  return formatTeam(team);
}
