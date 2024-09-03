use std::array;

use crate::{trait_helpers::stack::StackKernel, DTModel};
use dfdx::prelude::*;
use num_traits::Float;
use rand_distr::uniform::SampleUniform;

use crate::{
    dt_model::{BatchedInput, Input},
    DTState,
    //sts::get_samples,
};

pub fn batch_inputs<
    const EPISODES_IN_SEQ: usize,
    const B: usize,
    const S: usize,
    const A: usize,
    E: Dtype,
    D: Device<E> + StackKernel<usize> + dfdx::tensor::ZerosTensor<usize>,
>(
    inputs: [Input<EPISODES_IN_SEQ, S, A, E, D, NoneTape>; B],
    device: &D,
) -> BatchedInput<EPISODES_IN_SEQ, B, S, A, E, D, NoneTape> {
    let mut states = array::from_fn(|_| device.zeros());
    let mut actions = array::from_fn(|_| device.zeros());
    let mut rewards = array::from_fn(|_| device.zeros());
    let mut times = array::from_fn(|_| device.zeros());

    for (i, (state, action, reward, timestep)) in inputs.into_iter().enumerate() {
        states[i] = state;
        actions[i] = action;
        rewards[i] = reward;
        times[i] = timestep;
    }

    (
        states.stack(),
        actions.stack(),
        rewards.stack(),
        times.stack(),
    )
}

pub fn game_to_inputs<E: Dtype + From<f32>+ num_traits::Float + rand_distr::uniform::SampleUniform, D: Device<E> + dfdx::tensor::ZerosTensor<usize> + StackKernel<usize>, Game: DTState<E, D>>(
    states: Vec<Game>,
    actions: Vec<Game::Action>,
    dev: &D
) -> Vec<
    Input<
        { Game::EPISODES_IN_SEQ },
        { Game::STATE_SIZE },
        { Game::ACTION_SIZE },
        E,
        D,
        NoneTape,
    >,
>{
    let mut inputs = vec![];

    let rewards_to_go = get_rewards_to_go(&states, &actions);

    let mut actions_in_seq: [Tensor<(Const<{ Game::ACTION_SIZE }>,), E, D>; Game::EPISODES_IN_SEQ] =
        std::array::from_fn(|_| dev.zeros());
    let mut states_in_seq: [Tensor<(Const<{ Game::STATE_SIZE }>,), E, D>; Game::EPISODES_IN_SEQ] =
        std::array::from_fn(|_| dev.zeros());
    let mut rtg_in_seq: [Tensor<(Const<1>,), E, D>; Game::EPISODES_IN_SEQ] =
        std::array::from_fn(|_| dev.zeros());
    let mut timesteps_in_seq: [Tensor<(), usize, D>; Game::EPISODES_IN_SEQ] =
        std::array::from_fn(|_| dev.zeros());

    for (i, (state, action)) in states.into_iter().zip(actions.into_iter()).enumerate() {
        // Update actions
        let new_action = Game::action_to_tensor(&action);
        next_sequence(&mut actions_in_seq, new_action);

        // Update states
        let new_state = state.to_tensor();
        next_sequence(&mut states_in_seq, new_state);

        // Update rtg
        let new_reward: E = rewards_to_go[i].into();
        next_sequence(&mut rtg_in_seq, dev.tensor([new_reward]));

        // Update timesteps
        next_sequence(&mut timesteps_in_seq, dev.tensor(i + 1));

        let input = (
            states_in_seq.clone().stack(),
            masked_next(&actions_in_seq, dev).stack(),
            masked_next(&rtg_in_seq, dev).stack(),
            timesteps_in_seq.clone().stack(),
        );
        inputs.push(input)
    }

    inputs
}

fn next_sequence<
    E: Dtype + From<f32> + num_traits::Float + rand_distr::uniform::SampleUniform,
    D: Device<E>,
    Game: DTState<E, D>,
    T,
>(
    seq: &mut [T; Game::EPISODES_IN_SEQ],
    new_last_element: T,
) {
    seq.rotate_left(1);
    seq[seq.len() - 1] = new_last_element;
}

pub fn get_rewards_to_go<E: Dtype + From<f32>+ num_traits::Float + rand_distr::uniform::SampleUniform , D: Device<E>, Game: DTState<E, D>>(
    states: &Vec<Game>,
    actions: &Vec<Game::Action>,
) -> Vec<f32> {
    let mut backwards_rewards = vec![];
    let mut rewards_so_far = 0.0;
    assert_eq!(actions.len(), states.len());
    for i in 0..actions.len() {
        let reward = Game::get_reward(&states[i], actions[i].clone());
        rewards_so_far += reward;
        backwards_rewards.push(rewards_so_far);
    }
    backwards_rewards.reverse();
    backwards_rewards
}

fn masked_next<E: Dtype + From<f32>+ num_traits::Float + rand_distr::uniform::SampleUniform, D: Device<E>, Game: DTState<E, D>, S: ConstShape>(
    seq: &[Tensor<S, E, D>; Game::EPISODES_IN_SEQ],
    dev: &D,
) -> [Tensor<S, E, D>; Game::EPISODES_IN_SEQ]{
    let mut new_seq = seq.clone();
    new_seq[new_seq.len() - 1] = dev.zeros();
    new_seq
}

pub struct DTModelWrapper<
    E: Dtype + From<f32> + Float + SampleUniform,
    D: Device<E>,
    Game: DTState<E, D>,
>(pub DTModel<{ Game::MAX_EPISODES_IN_GAME }, { Game::STATE_SIZE }, { Game::ACTION_SIZE }, E, D>)
where
    [(); Game::MAX_EPISODES_IN_GAME]: Sized,
    [(); Game::ACTION_SIZE]: Sized,
    [(); Game::STATE_SIZE]: Sized;
