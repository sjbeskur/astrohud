use actix_web::{web, HttpResponse, Responder};
use crate::app_state::{AppState, Todo};

// pub async fn get_image_count(data: web::Data<AppState>) -> impl Responder {
//     let images = data.images.lock().unwrap();
//     HttpResponse::Ok().body(format!("Number of images: {}", images.len()))
// }

// pub async fn get_latest_image(data: web::Data<AppState>) -> impl Responder {
//     let images = data.images.lock().unwrap();
//     match images.last() {
//         Some(image) => HttpResponse::Ok().content_type("image/jpeg").body(image.clone()),
//         None => HttpResponse::NotFound().body("No images available"),
//     }
// }

/*
 
// Todo Routes
// Handler to get all todos
pub async fn get_todos(data: web::Data<AppState>) -> impl Responder {
    let todos = data.todos.lock().unwrap();
    HttpResponse::Ok().json(&*todos)
}


// Handler to get a specific todo
pub async fn get_todo(
    data: web::Data<AppState>,
    path: web::Path<u32>,
) -> impl Responder {
    let todos = data.todos.lock().unwrap();
    match todos.iter().find(|todo| todo.id == *path) {
        Some(todo) => HttpResponse::Ok().json(todo),
        None => HttpResponse::NotFound().body("Todo not found"),
    }
}

// Handler to create a new todo
pub async fn create_todo(
    data: web::Data<AppState>,
    todo: web::Json<Todo>,
) -> impl Responder {
    let mut todos = data.todos.lock().unwrap();
    let new_todo = Todo {
        id: (todos.len() as u32) + 1,
        title: todo.title.clone(),
        completed: todo.completed,
    };
    todos.push(new_todo.clone());
    HttpResponse::Ok().json(new_todo)
}

// Handler to update a todo
pub async fn update_todo(
    data: web::Data<AppState>,
    path: web::Path<u32>,
    todo: web::Json<Todo>,
) -> impl Responder {
    let mut todos = data.todos.lock().unwrap();
    match todos.iter_mut().find(|t| t.id == *path) {
        Some(existing_todo) => {
            existing_todo.title = todo.title.clone();
            existing_todo.completed = todo.completed;
            HttpResponse::Ok().json(existing_todo)
        }
        None => HttpResponse::NotFound().body("Todo not found"),
    }
}
// Handler to delete a todo
pub async fn delete_todo(
    data: web::Data<AppState>,
    path: web::Path<u32>,
) -> impl Responder {
    let mut todos = data.todos.lock().unwrap();
    let initial_len = todos.len();
    todos.retain(|todo| todo.id != *path);
    
    if todos.len() < initial_len {
        HttpResponse::Ok().body("Todo deleted")
    } else {
        HttpResponse::NotFound().body("Todo not found")
    }
}

*/