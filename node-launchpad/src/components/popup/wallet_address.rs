// Copyright 2024 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use super::super::utils::centered_rect_fixed;
use super::super::Component;
use crate::{
    action::{Action, OptionsActions},
    mode::{InputMode, Scene},
    style::{clear_area, EUCALYPTUS, GHOST_WHITE, INDIGO, LIGHT_PERIWINKLE, VIVID_SKY_BLUE},
    widgets::hyperlink::Hyperlink,
};
use color_eyre::Result;
use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::{prelude::*, widgets::*};
use tui_input::{backend::crossterm::EventHandler, Input};

const INPUT_SIZE_USERNAME: u16 = 32; // as per discord docs
const INPUT_AREA_USERNAME: u16 = INPUT_SIZE_USERNAME + 2; // +2 for the padding

pub struct EvmInfo {
    /// Whether the component is active right now, capturing keystrokes + draw things.
    active: bool,
    state: WalletInfoState,
    wallet_input_filed: Input,
    // cache the old value incase user presses Esc.
    old_value: String,
    back_to: Scene,
}

enum WalletInfoState {
    EnterAddress,
}

impl EvmInfo {
    pub fn new(wallet: String) -> Self {
        let state = WalletInfoState::EnterAddress;

        Self {
            active: false,
            state,
            wallet_input_filed: Input::default().with_value(wallet),
            old_value: Default::default(),
            back_to: Scene::Status,
        }
    }

    fn capture_inputs(&mut self, key: KeyEvent) -> Vec<Action> {
        let send_back = match key.code {
            KeyCode::Enter => {
                let wallet_address = self.wallet_input_filed.value().to_string().to_lowercase();
                self.wallet_input_filed = wallet_address.clone().into();

                debug!(
                    "Got Enter, saving the wallet address {wallet_address:?} and switching to WalletAlreadySet, and Home Scene",
                );
                vec![
                    Action::StoreWalletAddress(wallet_address.clone()),
                    Action::OptionsActions(OptionsActions::UpdateWalletInfoAddress(wallet_address)),
                    Action::SwitchScene(self.back_to),
                ]
            }
            KeyCode::Esc => {
                debug!(
                    "Got Esc, restoring the old value {} and switching to actual screen",
                    self.old_value
                );
                // reset to old value
                self.wallet_input_filed = self
                    .wallet_input_filed
                    .clone()
                    .with_value(self.old_value.clone());
                vec![Action::SwitchScene(self.back_to)]
            }
            KeyCode::Char(' ') => vec![],
            KeyCode::Backspace => {
                // if max limit reached, we should allow Backspace to work.
                self.wallet_input_filed.handle_event(&Event::Key(key));
                vec![]
            }
            _ => {
                // max 32 limit as per discord docs
                if self.wallet_input_filed.value().chars().count() < 32 {
                    self.wallet_input_filed.handle_event(&Event::Key(key));
                }
                vec![]
            }
        };
        send_back
    }
}

impl Component for EvmInfo {
    fn handle_key_events(&mut self, key: KeyEvent) -> Result<Vec<Action>> {
        if !self.active {
            return Ok(vec![]);
        }
        // while in entry mode, keybinds are not captured, so gotta exit entry mode from here
        let send_back = self.capture_inputs(key);
        Ok(send_back)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        let send_back = match action {
            Action::SwitchScene(scene) => match scene {
                Scene::StatusWalletInfoPopUp | Scene::OptionsWalletInfoPopUp => {
                    self.active = true;
                    self.old_value = self.wallet_input_filed.value().to_string();
                    if scene == Scene::StatusWalletInfoPopUp {
                        self.back_to = Scene::Status;
                    } else if scene == Scene::OptionsWalletInfoPopUp {
                        self.back_to = Scene::Options;
                    }
                    // Set to InputMode::Entry as we want to handle everything within our handle_key_events
                    // so by default if this scene is active, we capture inputs.
                    Some(Action::SwitchInputMode(InputMode::Entry))
                }
                _ => {
                    self.active = false;
                    None
                }
            },
            _ => None,
        };
        Ok(send_back)
    }

    fn draw(&mut self, f: &mut crate::tui::Frame<'_>, area: Rect) -> Result<()> {
        if !self.active {
            return Ok(());
        }

        let layer_zero = centered_rect_fixed(52, 15, area);

        let layer_one = Layout::new(
            Direction::Vertical,
            [
                // for the pop_up_border
                Constraint::Length(2),
                // for the input field
                Constraint::Min(1),
                // for the pop_up_border
                Constraint::Length(1),
            ],
        )
        .split(layer_zero);

        // layer zero
        let pop_up_border = Paragraph::new("").block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Add your wallet ")
                .bold()
                .title_style(Style::new().fg(VIVID_SKY_BLUE))
                .padding(Padding::uniform(2))
                .border_style(Style::new().fg(VIVID_SKY_BLUE)),
        );
        clear_area(f, layer_zero);

        match self.state {
            WalletInfoState::EnterAddress => {
                // split into 4 parts, for the prompt, input, text, dash , and buttons
                let layer_two = Layout::new(
                    Direction::Vertical,
                    [
                        // for the prompt text
                        Constraint::Length(3),
                        // for the input
                        Constraint::Length(1),
                        // for the text
                        Constraint::Length(6),
                        // gap
                        Constraint::Length(1),
                        // for the buttons
                        Constraint::Length(1),
                    ],
                )
                .split(layer_one[1]);

                let prompt_text = Paragraph::new("Enter new wallet address:")
                    .block(Block::default())
                    .alignment(Alignment::Center)
                    .fg(GHOST_WHITE);

                f.render_widget(prompt_text, layer_two[0]);

                let spaces = " ".repeat(
                    (INPUT_AREA_USERNAME - 1) as usize - self.wallet_input_filed.value().len(),
                );
                let input = Paragraph::new(Span::styled(
                    format!("{}{} ", spaces, self.wallet_input_filed.value()),
                    Style::default().fg(VIVID_SKY_BLUE).bg(INDIGO).underlined(),
                ))
                .alignment(Alignment::Center);
                f.render_widget(input, layer_two[1]);

                let text = Paragraph::new(Text::from(vec![
                    Line::raw("Changing your wallet address will reset all nodes,"),
                    Line::raw("and any Attos left on this device will be lost."),
                ]))
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .padding(Padding::horizontal(2))
                        .padding(Padding::top(2)),
                );

                f.render_widget(text.fg(GHOST_WHITE), layer_two[2]);

                let dash = Block::new()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::new().fg(GHOST_WHITE));
                f.render_widget(dash, layer_two[3]);

                let buttons_layer = Layout::horizontal(vec![
                    Constraint::Percentage(55),
                    Constraint::Percentage(45),
                ])
                .split(layer_two[4]);

                let button_no = Line::from(vec![Span::styled(
                    "  Cancel [Esc]",
                    Style::default().fg(LIGHT_PERIWINKLE),
                )]);

                f.render_widget(button_no, buttons_layer[0]);

                let button_yes = Line::from(vec![Span::styled(
                    "Save Wallet [Enter]",
                    Style::default().fg(EUCALYPTUS),
                )]);
                f.render_widget(button_yes, buttons_layer[1]);
            }
        }

        f.render_widget(pop_up_border, layer_zero);

        Ok(())
    }
}
