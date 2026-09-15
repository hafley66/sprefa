import { TeamRecord, UserRecord } from "./records";

export function formatUser(record: UserRecord): string {
  return record.name;
}

export function formatTeam(team: TeamRecord): string {
  return formatUser(team.owner);
}
