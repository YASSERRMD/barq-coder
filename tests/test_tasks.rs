use barqcoder::tasks::{TaskBoard, TaskStatus};
use std::sync::Arc;

#[test]
fn test_task_creation() {
    let board = TaskBoard::new();
    let res = board.create_task("T-1".to_string(), "Test task".to_string(), vec![]);
    assert!(res.is_ok());

    let task = board.get_task("T-1").unwrap();
    assert_eq!(task.id, "T-1");
    assert_eq!(task.description, "Test task");
    assert_eq!(task.status, TaskStatus::Pending);
}

#[test]
fn test_task_dependencies() {
    let board = TaskBoard::new();
    
    // Create base tasks
    board.create_task("DEP-1".to_string(), "Dependency 1".to_string(), vec![]).unwrap();
    board.create_task("DEP-2".to_string(), "Dependency 2".to_string(), vec![]).unwrap();

    // Create dependent task
    board.create_task(
        "MAIN".to_string(), 
        "Main task".to_string(), 
        vec!["DEP-1".to_string(), "DEP-2".to_string()]
    ).unwrap();

    // Dependencies not met yet
    assert_eq!(board.are_dependencies_met("MAIN").unwrap(), false);

    // Complete one dependency
    board.update_status("DEP-1", TaskStatus::Completed).unwrap();
    assert_eq!(board.are_dependencies_met("MAIN").unwrap(), false);

    // Complete second dependency
    board.update_status("DEP-2", TaskStatus::Completed).unwrap();
    assert_eq!(board.are_dependencies_met("MAIN").unwrap(), true);
}

#[test]
fn test_task_assignment_and_update() {
    let board = TaskBoard::new();
    board.create_task("T-1".to_string(), "Test assign".to_string(), vec![]).unwrap();

    let task = board.get_task("T-1").unwrap();
    assert!(task.assigned_to.is_none());

    // Assign task
    board.assign_task("T-1", "CoderAgent").unwrap();
    let task = board.get_task("T-1").unwrap();
    assert_eq!(task.assigned_to.unwrap(), "CoderAgent");

    // Update status
    board.update_status("T-1", TaskStatus::InProgress).unwrap();
    let task = board.get_task("T-1").unwrap();
    assert_eq!(task.status, TaskStatus::InProgress);
}
