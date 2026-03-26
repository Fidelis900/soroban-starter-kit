import React, { useState } from 'react';
import { useMarketplace } from '../../context/MarketplaceContext';
import { Integration, Review } from './types';

interface Props {
  integrationId: string;
}

export const ReviewSection: React.FC<Props> = ({ integrationId }) => {
  const { getReviewsForIntegration, submitReview, isLoading } = useMarketplace();
  const reviews = getReviewsForIntegration(integrationId);
  const [isFormOpen, setIsFormOpen] = useState(false);
  const [rating, setRating] = useState(5);
  const [comment, setComment] = useState('');

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!comment) return;
    
    await submitReview({
      integrationId,
      user: 'You', // In a real app, this would be the logged-in user
      rating,
      comment
    });
    
    setComment('');
    setIsFormOpen(false);
  };

  return (
    <div className="review-section mt-lg">
      <div className="flex justify-between items-center mb-md">
        <h4 className="m-0">User Reviews ({reviews.length})</h4>
        <button 
          className="btn btn-secondary btn-sm"
          onClick={() => setIsFormOpen(!isFormOpen)}
        >
          {isFormOpen ? 'Cancel' : 'Write a Review'}
        </button>
      </div>

      {isFormOpen && (
        <form onSubmit={handleSubmit} className="card glass-effect mb-md p-md">
          <div className="form-group">
            <label className="form-label">Rating</label>
            <div className="flex gap-sm">
              {[1, 2, 3, 4, 5].map(star => (
                <button
                  key={star}
                  type="button"
                  onClick={() => setRating(star)}
                  style={{ 
                    background: 'none', 
                    border: 'none', 
                    cursor: 'pointer',
                    fontSize: '1.5rem',
                    color: star <= rating ? 'var(--color-warning)' : 'var(--color-text-muted)'
                  }}
                >
                  ★
                </button>
              ))}
            </div>
          </div>
          <div className="form-group">
            <label className="form-label">Comment</label>
            <textarea
              className="form-input"
              rows={3}
              value={comment}
              onChange={e => setComment(e.target.value)}
              placeholder="What do you think about this integration?"
              required
            />
          </div>
          <button type="submit" disabled={isLoading} className="btn btn-primary w-full">
            {isLoading ? 'Submitting...' : 'Post Review'}
          </button>
        </form>
      )}

      <div className="reviews-list flex flex-col gap-md">
        {reviews.length === 0 ? (
          <p className="text-muted italic">No reviews yet. Be the first to review!</p>
        ) : (
          reviews.map(review => (
            <div key={review.id} className="review-item p-md" style={{ borderBottom: '1px solid var(--color-border)' }}>
              <div className="flex justify-between items-center mb-xs">
                <span className="font-bold">{review.user}</span>
                <span className="text-warning">{'★'.repeat(review.rating)}{'☆'.repeat(5-review.rating)}</span>
              </div>
              <p className="m-0 text-sm text-secondary">{review.comment}</p>
              <span className="text-muted" style={{ fontSize: '0.75rem' }}>
                {new Date(review.date).toLocaleDateString()}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
