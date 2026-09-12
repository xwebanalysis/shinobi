import { Routes } from '@angular/router';

import { DashboardComponent } from './features/dashboard/dashboard';
import { FilesComponent } from './features/files/files';
import { HistoryComponent } from './features/history/history';
import { SchedulesComponent } from './features/schedules/schedules';

export const routes: Routes = [
  { path: '', component: DashboardComponent },
  { path: 'history', component: HistoryComponent },
  { path: 'schedules', component: SchedulesComponent },
  { path: 'files', component: FilesComponent },
  { path: '**', redirectTo: '' },
];
