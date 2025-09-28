import React from "react";

interface DeleteButtonProps {
  cookie: string;
  onDelete: (cookie: string) => void;
  isDeleting: boolean;
}

const DeleteButton: React.FC<DeleteButtonProps> = ({
  cookie,
  onDelete,
  isDeleting,
}) => {
  return (
    <button
      onClick={() => onDelete(cookie)}
      disabled={isDeleting}
      className={`ml-2 p-1 rounded-md transition-colors ${
        isDeleting
          ? "bg-gray-700 text-gray-400 cursor-not-allowed"
          : "text-red-400 hover:text-red-300 hover:bg-red-900/30"
      }`}
      title="Delete cookie"
    >
      {isDeleting ? (
        <svg
          className="animate-spin h-4 w-4"
          xmlns="http://www.w3.org/2000/svg"
          fill="none"
          viewBox="0 0 24 24"
        >
          <circle
            className="opacity-25"
            cx="12"
            cy="12"
            r="10"
            stroke="currentColor"
            strokeWidth="4"
          ></circle>
          <path
            className="opacity-75"
            fill="currentColor"
            d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
          ></path>
        </svg>
      ) : (
        <svg
          xmlns="http://www.w3.org/2000/svg"
          className="h-4 w-4"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
          />
        </svg>
      )}
    </button>
  );
};

export default DeleteButton;
